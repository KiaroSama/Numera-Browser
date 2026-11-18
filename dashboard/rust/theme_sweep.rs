//! Glimm's silver theme sweep, rendered by the browser and driven by Rust.
use js_sys::{Float32Array, Function, Promise, Reflect};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    Document, HtmlCanvasElement, HtmlElement, WebGlRenderingContext as Gl, WebGlShader, Window,
};

const SWEEP_MS: f64 = 1100.0;
const OUTRO_MS: f64 = 220.0;
const ATTRIBUTE: &str = "data-theme-sweep";
const CUT_PROPERTY: &str = "--theme-sweep-cut";
type Commit = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

thread_local! {
    static ACTIVE: RefCell<Option<Sweep>> = const { RefCell::new(None) };
}

struct Sweep {
    root: HtmlElement,
    canvas: HtmlCanvasElement,
    transition: JsValue,
    commit: Commit,
}

impl Drop for Sweep {
    fn drop(&mut self) {
        if let Some(skip) = member::<Function>(&self.transition, "skipTransition") {
            let _ = skip.call0(&self.transition);
        }
        let _ = self.root.remove_attribute(ATTRIBUTE);
        let _ = self.root.style().remove_property(CUT_PROPERTY);
        self.canvas.remove();
    }
}

fn member<T: JsCast>(object: &JsValue, name: &str) -> Option<T> {
    Reflect::get(object, &JsValue::from_str(name))
        .ok()?
        .dyn_into()
        .ok()
}

fn apply(commit: &Commit) {
    let update = commit.borrow_mut().take();
    if let Some(update) = update {
        update();
    }
}

pub fn stop(settle: bool) {
    let sweep = ACTIVE.with(|active| active.borrow_mut().take());
    if let Some(sweep) = sweep {
        if settle {
            apply(&sweep.commit);
        } else {
            sweep.commit.borrow_mut().take();
        }
    }
}

pub fn play(
    window: &Window,
    document: &Document,
    from_dark: bool,
    update: impl FnOnce() + 'static,
) {
    let commit: Commit = Rc::new(RefCell::new(Some(Box::new(update))));
    if !start(window, document, from_dark, &commit) {
        apply(&commit);
    }
}

fn start(window: &Window, document: &Document, from_dark: bool, commit: &Commit) -> bool {
    if document.hidden()
        || window
            .match_media("(prefers-reduced-motion: reduce)")
            .ok()
            .flatten()
            .is_some_and(|query| query.matches())
    {
        return false;
    }
    let Some((start, root, canvas, gl)) = setup(document, from_dark) else {
        return false;
    };
    let pending = commit.clone();
    let callback = Closure::once(move || apply(&pending));
    let Ok(transition) = start.call1(document.as_ref(), callback.as_ref()) else {
        canvas.remove();
        let _ = root.remove_attribute(ATTRIBUTE);
        return false;
    };
    let ready = member::<Promise>(&transition, "ready").map(JsFuture::from);
    let updated = member::<Promise>(&transition, "updateCallbackDone").map(JsFuture::from);
    let finished = member::<Promise>(&transition, "finished").map(JsFuture::from);
    ACTIVE.with(|active| {
        *active.borrow_mut() = Some(Sweep {
            root: root.clone(),
            canvas: canvas.clone(),
            transition,
            commit: commit.clone(),
        });
    });
    let window = window.clone();
    let commit = commit.clone();
    spawn_local(async move {
        if let Some(ready) = ready
            && ready.await.is_ok()
        {
            animate(&window, &root, &canvas, &gl, &commit).await;
        }
        if is_active(&commit) {
            stop(true);
        }
        // The browser may still call the update after a skipped transition.
        if let Some(updated) = updated {
            let _ = updated.await;
        }
        drop(callback);
        if let Some(finished) = finished {
            let _ = finished.await;
        }
    });
    true
}

fn setup(
    document: &Document,
    from_dark: bool,
) -> Option<(Function, HtmlElement, HtmlCanvasElement, Gl)> {
    let start = member(document.as_ref(), "startViewTransition")?;
    let root: HtmlElement = document.document_element()?.dyn_into().ok()?;
    let canvas: HtmlCanvasElement = document.create_element("canvas").ok()?.dyn_into().ok()?;
    canvas.set_width(0);
    canvas.set_height(0);
    canvas.set_class_name("ps-theme-sweep");
    canvas.set_attribute("aria-hidden", "true").ok()?;
    let gl: Gl = canvas.get_context("webgl").ok()??.dyn_into().ok()?;
    let vertex = compile(&gl, Gl::VERTEX_SHADER, VERTEX)?;
    let fragment = compile(&gl, Gl::FRAGMENT_SHADER, SILVER)?;
    let program = gl.create_program()?;
    gl.attach_shader(&program, &vertex);
    gl.attach_shader(&program, &fragment);
    gl.link_program(&program);
    gl.delete_shader(Some(&vertex));
    gl.delete_shader(Some(&fragment));
    if !gl
        .get_program_parameter(&program, Gl::LINK_STATUS)
        .is_truthy()
    {
        return None;
    }
    gl.use_program(Some(&program));
    let corners = gl.create_buffer()?;
    gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&corners));
    let vertices = Float32Array::from(&[-1.0_f32, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0][..]);
    gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &vertices, Gl::STATIC_DRAW);
    let position = u32::try_from(gl.get_attrib_location(&program, "position")).ok()?;
    gl.enable_vertex_attrib_array(position);
    gl.vertex_attrib_pointer_with_i32(position, 2, Gl::FLOAT, false, 0, 0);
    gl.uniform1f(
        gl.get_uniform_location(&program, "direction").as_ref(),
        if from_dark { -1.0 } else { 1.0 },
    );
    gl.uniform1f(
        gl.get_uniform_location(&program, "brightness").as_ref(),
        if from_dark { 0.85 } else { 1.0 },
    );
    document.body()?.append_child(&canvas).ok()?;
    if root
        .set_attribute(ATTRIBUTE, if from_dark { "rtl" } else { "ltr" })
        .is_err()
    {
        canvas.remove();
        return None;
    }
    Some((start, root, canvas, gl))
}

fn compile(gl: &Gl, kind: u32, source: &str) -> Option<WebGlShader> {
    let shader = gl.create_shader(kind)?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);
    if gl
        .get_shader_parameter(&shader, Gl::COMPILE_STATUS)
        .is_truthy()
    {
        Some(shader)
    } else {
        gl.delete_shader(Some(&shader));
        None
    }
}

fn is_active(commit: &Commit) -> bool {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .is_some_and(|sweep| Rc::ptr_eq(&sweep.commit, commit))
    })
}

async fn animate(
    window: &Window,
    root: &HtmlElement,
    canvas: &HtmlCanvasElement,
    gl: &Gl,
    commit: &Commit,
) {
    let Some(program) = gl
        .get_parameter(Gl::CURRENT_PROGRAM)
        .ok()
        .and_then(|v| v.dyn_into().ok())
    else {
        return;
    };
    let resolution = gl.get_uniform_location(&program, "resolution");
    let progress_uniform = gl.get_uniform_location(&program, "progress");
    let alpha = gl.get_uniform_location(&program, "alpha");
    let time = gl.get_uniform_location(&program, "seconds");
    let mut first = None;
    while let Some(frame) = frame(window).await {
        if !is_active(commit) || gl.is_context_lost() {
            break;
        }
        let elapsed = frame - *first.get_or_insert(frame);
        if elapsed >= SWEEP_MS + OUTRO_MS {
            break;
        }
        let factor = (elapsed / SWEEP_MS).clamp(0.0, 1.0);
        let progress = ease(factor);
        let cut = (progress * 1.4 - 0.2).clamp(0.0, 1.0);
        let _ = root
            .style()
            .set_property(CUT_PROPERTY, &format!("{}%", 100.0 * cut));
        let width = root.client_width().max(1);
        let height = root.client_height().max(1);
        let scale = (1920.0 / f64::from(width.max(height))).min(1.0);
        let size = (
            (f64::from(width) * scale).round().max(1.0) as u32,
            (f64::from(height) * scale).round().max(1.0) as u32,
        );
        if (canvas.width(), canvas.height()) != size {
            canvas.set_width(size.0);
            canvas.set_height(size.1);
            gl.viewport(0, 0, size.0 as i32, size.1 as i32);
            gl.uniform2f(resolution.as_ref(), size.0 as f32, size.1 as f32);
        }
        gl.uniform1f(progress_uniform.as_ref(), progress as f32);
        gl.uniform1f(
            alpha.as_ref(),
            (1.0 - (elapsed - SWEEP_MS).max(0.0) / OUTRO_MS) as f32,
        );
        gl.uniform1f(time.as_ref(), (elapsed / 1000.0) as f32);
        gl.draw_arrays(Gl::TRIANGLE_STRIP, 0, 4);
    }
}

async fn frame(window: &Window) -> Option<f64> {
    JsFuture::from(Promise::new(&mut |resolve, reject| {
        if let Err(error) = window.request_animation_frame(&resolve) {
            let _ = reject.call1(&JsValue::UNDEFINED, &error);
        }
    }))
    .await
    .ok()?
    .as_f64()
}

// Invert the x component of the CSS ease curve (0.25, 0.1, 0.25, 1).
fn ease(progress: f64) -> f64 {
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..12 {
        let t = (low + high) / 2.0;
        let x = 0.75 * (1.0 - t) * t + t * t * t;
        if x < progress {
            low = t;
        } else {
            high = t;
        }
    }
    let t = (low + high) / 2.0;
    0.3 * (1.0 - t) * (1.0 - t) * t + 3.0 * (1.0 - t) * t * t + t * t * t
}

const VERTEX: &str = "
attribute vec2 position;
void main() { gl_Position = vec4(position, 0.0, 1.0); }
";

// Glimm 0.4.1 (MIT). Horizontal, full-viewport sweep using GodVerify's silver palette.
const SILVER: &str = r#"
precision mediump float;
uniform vec2 resolution;
uniform float direction, brightness, progress, alpha, seconds;

vec3 silver(float t) {
    return vec3(0.74, 0.75, 0.78) + 0.24 * cos(6.2831853 * (vec3(t) + vec3(0.0, 0.02, 0.05)));
}

void main() {
    vec2 uv = gl_FragCoord.xy / resolution;
    float center = 0.5 + direction * (1.4 * progress - 0.7);
    float d = uv.x - center;
    float band = exp(-d * d * 14.0);
    vec3 normal = normalize(vec3(5.04 * d * band, 0.0, 1.0));
    float behind = max(-d * direction, 0.0);
    float wake = smoothstep(0.0, 0.08, behind) * (1.0 - smoothstep(0.12, 0.65, behind)) * 0.10;
    float focus = 4.0 * progress * (1.0 - progress);
    float halo = exp(-d * d * 2.5) * 0.05 * focus;
    float intensity = clamp(max(band, wake) + halo * (1.0 - band), 0.0, 1.0);
    float ripple = sin(uv.y * 12.0 + uv.x * 3.0 + seconds * 0.40) * 0.015;
    float t = normal.x * 0.12 + uv.x * 0.90 + uv.y * 0.16 + ripple + seconds * 0.04;
    vec3 color = (silver(t) * 0.50 + silver(t - 0.18) * 0.25 + silver(t + 0.18) * 0.25) * brightness;
    vec3 halfLight = normalize(normalize(vec3(0.35, 0.55, 0.9)) + vec3(0.0, 0.0, 1.0));
    float fresnel = pow(1.0 - clamp(normal.z, 0.0, 1.0), 3.0);
    float specular = pow(clamp(dot(normal, halfLight), 0.0, 1.0), 80.0);
    float visibility = alpha * mix(0.2, 1.0, focus);
    float body = intensity * visibility;
    float highlight = band * visibility * 0.55;
    vec3 light = (color * fresnel * 0.55 + vec3(specular) * 1.1) * highlight;
    gl_FragColor = vec4(color * body + light, min(body + (fresnel * 0.4 + specular * 0.9) * highlight, 1.0));
}
"#;
