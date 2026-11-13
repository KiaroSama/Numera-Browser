# Build the exact native server first, from a clean checkout:
# cargo xtask build-product build/container --resources /path/to/resources --server-only
# The resource receipt inventories matched vendor drivers, browsers and notices.
# Only that reviewed product enters the runtime image; no source interpreter runs.
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates fontconfig fonts-liberation fonts-noto-color-emoji \
        libasound2t64 libatk-bridge2.0-0t64 libatk1.0-0t64 libatspi2.0-0t64 \
        libcairo2 libcups2t64 libdbus-1-3 libdrm2 libegl1 libgbm1 \
        libglib2.0-0t64 libgtk-3-0t64 libnspr4 libnss3 libpango-1.0-0 \
        libvulkan1 libx11-6 libx11-xcb1 libxcb1 libxcomposite1 libxcursor1 \
        libxdamage1 libxext6 libxfixes3 libxi6 libxkbcommon0 libxrandr2 \
        libxrender1 libxss1 libxt6 libxtst6 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 1001 numera \
    && useradd --uid 1001 --gid 1001 --create-home numera \
    && mkdir /data \
    && chown 1001:1001 /data

WORKDIR /app
# This directory includes web/, drivers/, browsers/, sources/, FILES.json,
# LICENSE, docs/licensing.md, docs/browser-notices.txt,
# docs/migration-random-notices.txt and
# engine-rust/src/http/documentation-assets/NOTICE.md.
COPY ["build/container/Numera Browser/engine/", "/app/"]
RUN ["/app/numera-engine", "--verify-installation"]

ENV NUMERA_DATA_DIR=/data \
    NUMERA_BROWSER_RUNTIME=/app/numera-browser-runtime \
    NUMERA_BROWSER_DRIVERS=/app/drivers

USER numera
VOLUME ["/data"]
EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD ["/app/numera-engine", "--http-healthcheck"]
ENTRYPOINT ["/app/numera-engine", "--data-dir", "/data", "serve", "--host", "0.0.0.0", "--port", "8787"]
