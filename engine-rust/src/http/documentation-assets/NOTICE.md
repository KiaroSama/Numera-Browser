# Documentation asset provenance

These HTML templates are generated from FastAPI 0.142.3 through the retained
Numera application by `scripts/migration/documentation_cases.py`. The OpenAPI
schema is the original application's route contract, not an invented replacement
schema. `provenance.json` records framework versions and generated asset hashes.
The native server substitutes only the package version in `info.version`.

The differential documentation test compares the retained application's GET,
HEAD and POST responses and rejects generated asset drift. Refresh the assets
with the pinned source environment when the original route contracts change.
No Python application is invoked when the Rust server serves these assets.

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
