# Documentation asset provenance

These HTML templates are generated from FastAPI 0.142.3 through the retained
Numera application by `scripts/migration/documentation_cases.py`. The OpenAPI
schema in `openapi.json` is the original application's frozen route contract.
`provenance.json` records framework versions and generated asset hashes; these
historical assets and their hashes remain unchanged.

`openapi-native.json` is an authored additive contract for public routes mounted
by the native application after that historical snapshot. The build combines
it with the frozen contract and substitutes the current package version in
`info.version`. It refuses to replace any historical route, parameterized route
shape or component. Shell-private update transaction routes remain excluded.

The differential test preserves the historical GET, HEAD and POST cases, exact
asset bytes and complete historical schema projection. Native additions are
checked exactly, and GET/HEAD retain exact content-length checks for the
composed document. A separate served-router regression checks the known public
native routes, their actual allowed methods, references, approval contracts and
private-route exclusions. No Python application runs to serve these assets.

The HTML templates include material from FastAPI's `fastapi/openapi/docs.py`:
https://github.com/fastapi/fastapi/blob/0.142.3/fastapi/openapi/docs.py

The original project license is unchanged. FastAPI's accompanying notice follows:

The MIT License (MIT)

Copyright (c) 2018 Sebastián Ramírez

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
