"""Every name the dashboard uses at module scope must be imported.

This is not a style check. esbuild resolves module *specifiers*, not free
variables: `<ShieldAlert />` with no import builds cleanly and throws the moment
that component renders. During the App.tsx split five icons went missing exactly
that way, and two of them were inside the error boundary - the one component that
must never itself fail.

The rule the checker uses is deliberately permissive: a name counts as used if it
appears as a word in the file's code with comments removed and **string literals
left in**. Stripping strings was tried and cost hours - JSX prose is full of
apostrophes ("Chromium's", "don't"), so a quote-matching regex runs from one
apostrophe to the next and swallows the code between, which is how `<Download>`
twice looked unused. The asymmetry settles it: an unused import is tree-shaken
and free, a missing one blanks the page.
"""
import re
from pathlib import Path

import pytest

SRC = Path(__file__).resolve().parents[2] / "dashboard" / "src"

pytestmark = pytest.mark.skipif(not SRC.is_dir(), reason="dashboard sources not present")

# Free at module scope by definition.
AMBIENT = {
    "window", "document", "console", "localStorage", "sessionStorage", "navigator",
    "setTimeout", "clearTimeout", "setInterval", "clearInterval", "fetch", "Math",
    "Date", "JSON", "Object", "Array", "String", "Number", "Boolean", "Set", "Map",
    "Promise", "Error", "RegExp", "URL", "URLSearchParams", "Blob", "FormData",
    "Event", "Intl", "isNaN", "parseInt", "parseFloat", "encodeURIComponent",
    "decodeURIComponent", "structuredClone", "AbortController", "TextEncoder",
    "TextDecoder", "atob", "btoa", "crypto", "performance", "alert", "confirm",
    "prompt", "Infinity", "NaN", "React", "Fragment", "Image", "FileReader",
}


def sources():
    paths = sorted(p for p in SRC.rglob("*") if p.suffix in {".ts", ".tsx"})
    assert paths, "No TypeScript dashboard sources were discovered"
    return paths


def code_of(text):
    """Comments out, strings in - see the module docstring for why."""
    body = "\n".join(l for l in text.split("\n") if not l.lstrip().startswith("import "))
    body = re.sub(r"/\*.*?\*/", " ", body, flags=re.S)
    # Not preceded by ':' (a URL) or '\' (the end of a regex literal such as
    # /^https?:\/\//i, whose final two slashes otherwise read as a comment and
    # take the rest of the line - including a <Download /> - with them).
    return re.sub(r"(?<![:\\])//[^\n]*", " ", body)


def bindings(text):
    """Every name the file defines or imports, at any nesting depth."""
    names = set()
    for m in re.finditer(r"\b(?:function|class|const|let|var)\s+(\w+)", text):
        names.add(m.group(1))
    # Named imports, including the mixed form `import App, { ErrorBoundary }`.
    for m in re.finditer(r"import\s+(?:\w+\s*,\s*)?\{([^}]+)\}\s+from", text):
        names |= {n.strip().split(" as ")[-1] for n in m.group(1).split(",") if n.strip()}
    for m in re.finditer(r"import\s+(\w+)\s*(?:,|from)", text):
        names.add(m.group(1))
    # Destructuring, including a capitalised loop variable: `([k, Ic, label]) =>`.
    for m in re.finditer(r"\[([\w,\s]+?)\]", text):
        names |= {n.strip() for n in m.group(1).split(",") if n.strip()}
    for m in re.finditer(r"\(([\w,\s]+?)\)\s*=>", text):
        names |= {n.strip() for n in m.group(1).split(",") if n.strip()}
    return names


@pytest.mark.parametrize("path", [p for p in sources() if p.suffix == ".tsx"], ids=lambda p: p.name)
def test_every_component_used_is_imported(path):
    """A JSX tag with no binding is a white screen, and the build will not say so."""
    text = path.read_text(encoding="utf-8")
    tags = set(re.findall(r"<\s*([A-Z]\w*)(?=[\s/>])", code_of(text)))
    missing = sorted(tags - bindings(text) - AMBIENT)
    assert not missing, (
        f"{path.name} renders {missing} but never imports them. "
        "esbuild builds this and it throws at render.")


@pytest.mark.parametrize("path", sources(), ids=lambda p: p.name)
def test_no_import_is_dead(path):
    """The other direction, so the list stays honest as views move around."""
    text = path.read_text(encoding="utf-8")
    used = set(re.findall(r"\b(\w+)\b", code_of(text)))
    imported = set()
    for m in re.finditer(r'import \{([^}]+)\} from', text):
        imported |= {n.strip() for n in m.group(1).split(",") if n.strip()}
    assert not sorted(imported - used), \
        f"{path.name} imports {sorted(imported - used)} and never mentions them"


def test_no_module_imports_the_demo_data_except_the_app():
    """Sample rows must not be reachable from the real views.

    Demo mode exists so the UI is explorable with no engine. The failure it
    invites is fabricated profiles appearing in *live* mode, which happened once
    already (a hardcoded API port sent the app to demo data on any other port).
    Keeping the import surface to App.tsx means a view cannot render a fake row
    even by accident.
    """
    offenders = [p.name for p in sources()
                 if p.name != "App.tsx" and re.search(r'from "\.\.?/demoData"', p.read_text(encoding="utf-8"))]
    assert not offenders, f"these import sample data: {offenders}"


def test_no_import_cycles():
    """`App` -> views -> primitives -> leaf. A cycle here is a runtime undefined."""
    graph = {}
    for p in sources():
        deps = set()
        for spec in re.findall(r'from "(\.[^"]+)"', p.read_text(encoding="utf-8")):
            base = (p.parent / spec).resolve()
            for ext in (".ts", ".tsx", ""):
                cand = Path(str(base) + ext)
                if cand.is_file():
                    deps.add(cand)
                    break
        graph[p.resolve()] = deps

    def walk(node, path):
        if node in path:
            return path[path.index(node):] + [node]
        for d in sorted(graph.get(node, ()), key=str):
            found = walk(d, path + [node])
            if found:
                return found
        return None

    for start in graph:
        cycle = walk(start, [])
        assert not cycle, " -> ".join(c.name for c in cycle)
