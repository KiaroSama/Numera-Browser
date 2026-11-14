# Locale and GeoIP source data

`locale-data.json` preserves all 258 territories and every language population,
territory population and literacy percentage from Camoufox's
`python/src/camoufox/territoryInfo.xml` at commit
`079a6ce8249bab98063e5ac9241e3879a3e6578d`. This is the source launcher's CLDR
snapshot, with no replacement distributions or language subset. The XML notice is:

> Copyright © 1991-2023 Unicode, Inc.
> CLDR data files are interpreted according to the LDML specification
> (http://unicode.org/reports/tr35/)
> For terms of use, see https://www.unicode.org/copyright.html

The same JSON contains the complete language-tags 1.3.2 IANA registry snapshot
(File-Date 2024-05-16), reduced to the fields read by the native validator:
Subtag, Deprecated and Suppress-Script. The source `index.json` classification
is preserved. The validator retains the source package's validation rules,
including singleton handling and suppressed-script/deprecation errors. Explicit
scripts are validated, while the emitted script remains the language's
Suppress-Script, as in Camoufox `normalize_locale`.

`geoip-sources.json` preserves the default, source names, combined/separate URL
keys, deprecation flags and dotted record paths from the same revision's
`repos.yml`. The native reader selects the active `config.yml` under the
provisioned directory and reads `mmdb/<lowercase name>-combined.mmdb` or
`mmdb/<lowercase name>-ipv4.mmdb` / `-ipv6.mmdb`. It never downloads, refreshes,
activates or writes a source; those operations belong to the native resource
owner. Unknown saved source configurations retain their custom dotted paths,
matching the source. Incomplete geolocation records fail with a redacted error.

The native implementation is `../camoufox_geo.rs`. The retained browser-policy
case calls `tests/common/camoufox_geo_cases.rs`; it checks all 3,804 frozen locale
inputs and draws, the catalog digests, tiny synthetic dual-stack MMDB lookups,
active-source migration, custom paths, corrupt and missing records, explicit
locale/config precedence, WebRTC pins and proxy lookup failure without host
fallback. Runtime tests use no Python and require no external database/service.
Network attempts in this helper test target a loopback proxy that rejects all
six source echo requests. A second SOCKS4 fixture verifies all six native relay
handshakes preserve the decoded USERID and fail without a host route.

The reference fixtures execute only the locale source's function/class AST
with locally supplied imports, using numpy, PyYAML and language-tags==1.3.2.
The generator validates each weighted choice with NumPy, then injects the
uniform draw into its CDF algorithm, avoiding cross-language PRNG assumptions.
`tests/fixtures/camoufox-geo.json` records the six source hashes and generated
data hashes. Six source-AST preference cases cover UI language and geolocation
permission defaults. The 2,206-byte `.mmdb` contains only synthetic documentation addresses and an independently encoded search
tree; no MaxMind data is redistributed.

Camoufox code and source repositories data retain MPL-2.0 (see `LICENSE`).
The language-tags parser and registry extraction retain this MIT notice:

The MIT License (MIT)

Copyright (c) 2014-2023 Onroerend Erfgoed

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
