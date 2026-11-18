use numera_core::browser::adapter::Adapter;
use serde_json::{Map, Value, json};
use std::time::Duration;

/// Real browser checks under the site's strict CSP. The source's five seed-one
/// profiles share the frozen media/audio values below; GPU/font measurements
/// remain host-specific and are checked through stable readbacks and restores.
pub fn verify(adapter: &mut Adapter, config: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let expression = r#"async (font) => {
        const same=(a,b)=>a.length===b.length&&a.every((v,i)=>v===b[i]);
        const canvas=document.createElement('canvas');canvas.width=96;canvas.height=48;
        const ctx=canvas.getContext('2d');ctx.fillStyle='#5a43cf';ctx.fillRect(0,0,96,48);
        const pixels=Array.from(ctx.getImageData(0,0,96,48).data);
        const again=Array.from(ctx.getImageData(0,0,96,48).data);
        const url=canvas.toDataURL();const blob=await new Promise(resolve=>canvas.toBlob(resolve));
        const el=document.createElement('div');el.style.width='23.5px';el.style.height='14.25px';document.body.appendChild(el);
        const one=el.getBoundingClientRect(),two=el.getBoundingClientRect();
        const rectStable=['x','y','width','height','top','right','bottom','left'].every(k=>one[k]===two[k]);el.remove();
        const buffer=new AudioBuffer({length:1201,numberOfChannels:2,sampleRate:44100});
        buffer.copyToChannel(new Float32Array(1201).fill(0.5),0);buffer.copyToChannel(new Float32Array(1201).fill(0.25),1);
        const a=buffer.getChannelData(0),b=buffer.getChannelData(1);const before=a[0];
        const devices=await navigator.mediaDevices.enumerateDevices(),devicesAgain=await navigator.mediaDevices.enumerateDevices();
        ctx.font='27px "Numera Definitely Missing Font",monospace';const originalFont=ctx.font;
        const hidden=ctx.measureText('mmmmmmmmmmlli').width;const restored=ctx.font===originalFont;
        ctx.font='27px monospace';const fallback=ctx.measureText('mmmmmmmmmmlli').width;
        const pc=new RTCPeerConnection({iceServers:[]});const received=[];
        const fn=e=>received.push(e.candidate?.candidate||'end');pc.addEventListener('icecandidate',fn);
        const lines=['candidate:a 1 udp 1 8.8.8.8 9 typ host','candidate:a 1 udp 1 192.168.1.2 9 typ host','candidate:a 1 udp 1 2001:4860::1 9 typ host','candidate:a 1 udp 1 fe80::1 9 typ host','candidate:a 1 udp 1 peer.local 9 typ host',''];
        const dispatch=line=>{const e=new Event('icecandidate');Object.defineProperty(e,'candidate',{value:line?{candidate:line}:null});pc.dispatchEvent(e)};
        for(const line of lines)dispatch(line);pc.removeEventListener('icecandidate',fn);dispatch(lines[1]);pc.close();
        const platform=Object.getOwnPropertyDescriptor(Navigator.prototype,'platform');
        return {first:globalThis.firstIdentity,csp:globalThis.identityCsp,artifacts:['wasm_bindgen','__numeraBrowserPage'].filter(k=>k in globalThis),
            languages:{frozen:Object.isFrozen(navigator.languages),fresh:navigator.languages!==navigator.languages},
            getter:{own:Object.hasOwn(navigator,'platform'),name:platform.get.name,length:platform.get.length,enumerable:platform.enumerable,configurable:platform.configurable,native:Function.prototype.toString.call(platform.get)},
            memory:'deviceMemory'in navigator?navigator.deviceMemory:null,touch:navigator.maxTouchPoints,touchSurface:'ontouchstart'in window,dnt:navigator.doNotTrack,
            canvas:{stable:same(pixels,again),stableUrl:url===canvas.toDataURL(),blob:blob.size>0,native:Function.prototype.toString.call(canvas.toDataURL)},
            fonts:{fallback:hidden===fallback,restored,native:Function.prototype.toString.call(document.fonts.check),whitespace:[document.fonts.check('\uFEFF27px "'+font+'"'),document.fonts.check('\u008527px "'+font+'"')]},rectStable,
            audio:{a:[a[0],a[1],a[500],a[1000]],b:[b[0],b[1],b[500],b[1000]],same:a===buffer.getChannelData(0),stable:before===buffer.getChannelData(0)[0]},
            media:{devices:devices.map(d=>({deviceId:d.deviceId,kind:d.kind,label:d.label,groupId:d.groupId,own:Object.keys(d),proto:Object.getPrototypeOf(d).constructor.name})),fresh:devices!==devicesAgain,same:devices.every((d,i)=>d===devicesAgain[i])},rtc:received};
    }"#;
    let font = config["fonts"]
        .as_array()
        .and_then(|fonts| fonts.first())
        .ok_or("Profile font fixture missing")?;
    let response = adapter.call(
        "evaluate",
        Map::from_iter([
            ("expression".into(), expression.into()),
            ("argument".into(), font.clone()),
        ]),
        Duration::from_secs(15),
    )?;
    assert_eq!(response["evaluated"], true);
    let view = &response["value"];
    assert_eq!(view["first"]["platform"], config["platform"]);
    assert_eq!(view["first"]["cores"], config["hardwareConcurrency"]);
    assert_eq!(view["first"]["languages"], config["languages"]);
    assert_eq!(
        view["first"]["dpr"].as_f64(),
        config["devicePixelRatio"].as_f64()
    );
    assert_eq!(view["first"]["webdriver"], false);
    assert_eq!(view["first"]["frozen"], true);
    assert_eq!(
        view["first"]["mask"],
        "function get platform() { [native code] }"
    );
    assert_eq!(view["csp"], json!({"eval":true,"func":true,"wasm":true}));
    assert_eq!(view["artifacts"], json!([]));
    assert_eq!(view["languages"], json!({"frozen":true,"fresh":true}));
    assert_eq!(
        view["getter"],
        json!({"own":false,"name":"","length":0,"enumerable":true,"configurable":true,"native":"function get platform() { [native code] }"})
    );
    if config["deviceMemory"].as_f64() == Some(0.0) {
        assert!(view["memory"].is_null());
    } else {
        assert_eq!(view["memory"].as_f64(), config["deviceMemory"].as_f64());
    }
    assert_eq!(view["touch"], config["maxTouchPoints"]);
    assert_eq!(
        view["touchSurface"],
        config["maxTouchPoints"].as_u64().is_some_and(|v| v > 0)
    );
    assert!(view["dnt"].is_null());
    assert_eq!(
        view["canvas"],
        json!({"stable":true,"stableUrl":true,"blob":true,"native":"function toDataURL() { [native code] }"})
    );
    assert_eq!(
        view["fonts"],
        json!({"fallback":true,"restored":true,"native":"function check() { [native code] }","whitespace":[true,false]})
    );
    assert_eq!(view["rectStable"], true);
    assert_eq!(config["seed"], 1);
    // Frozen from the original browser init script, independent of host GPU,
    // device availability, fonts, screen geometry and operating system.
    assert_eq!(
        view["audio"],
        json!({"a":[0.5000000596046448,0.5,0.5,0.5],"b":[0.2500000298023224,0.25,0.25,0.2500000298023224],"same":true,"stable":true})
    );
    let devices = view["media"]["devices"]
        .as_array()
        .ok_or("Media devices missing")?;
    let counts = [
        ("cameras", "videoinput"),
        ("microphones", "audioinput"),
        ("speakers", "audiooutput"),
    ];
    let mut kinds = Vec::new();
    for (key, kind) in counts {
        kinds.extend(std::iter::repeat_n(
            kind,
            config[key].as_u64().ok_or("Media count missing")? as usize,
        ));
    }
    assert_eq!(devices.len(), kinds.len());
    for (device, kind) in devices.iter().zip(kinds) {
        assert_eq!(device["kind"], kind);
        assert_eq!(device["label"], "");
        assert_eq!(device["own"], json!([]));
        assert_eq!(
            device["proto"],
            if kind == "audiooutput" {
                "MediaDeviceInfo"
            } else {
                "InputDeviceInfo"
            }
        );
        for key in ["deviceId", "groupId"] {
            let id = device[key].as_str().ok_or("Media ID missing")?;
            assert_eq!(id.len(), 64);
            assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
        }
    }
    assert_eq!(
        devices[0]["deviceId"],
        "358b05c80a469d3fd5a831152b964ce25f687febff25a2729d50f3d06052c629"
    );
    assert_eq!(
        devices[0]["groupId"],
        "ce00fc76afb762eba98069f5c4ef6650ab77d405a1433c899f74fba791fd0380"
    );
    assert_eq!(view["media"]["fresh"], true);
    assert_eq!(view["media"]["same"], true);
    assert_eq!(
        view["rtc"],
        json!([
            "candidate:a 1 udp 1 192.168.1.2 9 typ host",
            "candidate:a 1 udp 1 fe80::1 9 typ host",
            "candidate:a 1 udp 1 peer.local 9 typ host",
            "end"
        ])
    );
    Ok(())
}

/// wasm2js grows memory without detaching its previous ArrayBuffer. The generated
/// binding caches must refresh both byte and DataView views on the same live
/// module, including after Rust allocation grows it a second time.
pub fn memory_growth(adapter: &mut Adapter) -> Result<(), Box<dyn std::error::Error>> {
    let mut expression = String::from("async () => { const runtime = (");
    expression.push_str(numera_core::page_runtime::artifact());
    expression.push_str(r#")(); const memory=runtime.initSync().memory;
        document.body.textContent=JSON.stringify({tls:{ja3:'771,1-2-3,0,29,0',ja3_hash:'before-growth',ja4:'t13_before',grease:true}});
        const warm=await runtime.tls_probe();const before=memory.buffer.byteLength;
        memory.grow(2);const explicit=memory.buffer.byteLength;
        document.body.textContent=JSON.stringify({tls:{ja3:'771,4865-4866,0,29,0',ja3_hash:'after-growth-λ',ja4:'t13_after',grease:true},padding:'λ'.repeat(150000)});
        const probe=await runtime.tls_probe();
        return {warm,before,explicit,after:memory.buffer.byteLength,probe};
    }"#);
    let response = adapter.call(
        "evaluate",
        Map::from_iter([("expression".into(), expression.into())]),
        Duration::from_secs(15),
    )?;
    assert_eq!(response["evaluated"], true);
    let view = &response["value"];
    assert_eq!(view["warm"]["ja3Hash"], "before-growth");
    assert_eq!(view["warm"]["cipherCount"], 3);
    let before = view["before"]
        .as_u64()
        .ok_or("Initial module memory missing")?;
    let explicit = view["explicit"]
        .as_u64()
        .ok_or("Grown module memory missing")?;
    let after = view["after"]
        .as_u64()
        .ok_or("Final module memory missing")?;
    assert!(explicit > before);
    assert!(
        after > explicit,
        "Large Unicode input did not exercise Rust memory growth"
    );
    assert_eq!(view["probe"]["ja3Hash"], "after-growth-λ");
    assert_eq!(view["probe"]["ja4"], "t13_after");
    assert_eq!(view["probe"]["cipherCount"], 2);
    assert_eq!(view["probe"]["grease"], true);
    Ok(())
}
