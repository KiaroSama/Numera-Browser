# Numera Browser - container image
#
# Two stages. The first builds the dashboard with Node; the second is the
# Playwright Python image, which already carries Chromium and the ~90 system
# libraries it needs. Installing those onto a plain `python:slim` is possible
# and is a long, brittle apt list that breaks on every base-image bump - this
# is the same browser Playwright would fetch anyway, already matched to the
# client version.
#
# Build:  docker build -t numera-browser .
# Run:    see docker-compose.yml, or the Docker section of the README.

# --- 1. the dashboard -------------------------------------------------------
FROM node:26-slim AS dashboard

WORKDIR /src
# Copy the manifests alone first so `npm ci` is cached until they change.
COPY dashboard/package.json dashboard/package-lock.json ./
RUN npm ci
COPY dashboard/ ./
RUN npm run build


# --- 2. the engine ----------------------------------------------------------
# Pinned to the Playwright release that matches engine/pyproject.toml's client,
# because a browser build newer than its client is exactly how launches start
# failing with protocol errors. Held at 1.62: camoufox 0.5.7 requires
# playwright<1.63, and the desktop extra installs both.
FROM mcr.microsoft.com/playwright/python:v1.62.0-noble

ENV PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    NUMERA_DATA_DIR=/data \
    NUMERA_CONTAINER_PLAYWRIGHT_VERSION=1.62.0

WORKDIR /app

COPY engine/ ./engine/
# The dashboard is served by the engine itself, from inside the package, so it
# has to land where `dashboard_dir()` looks: numera/web next to the code.
COPY --from=dashboard /src/dist/ ./engine/numera/web/

# Keep the literal image tag visible to Dependabot. The contract test requires
# its version to equal this container-only pin; the general launch extra stays
# flexible for source installations. Resolve both requirements together.
RUN python -m pip install --no-cache-dir "./engine[api,launch,secure]" \
        "playwright==${NUMERA_CONTAINER_PLAYWRIGHT_VERSION}" \
    && python -m pip check

# `pwuser` ships with the Playwright image and owns the browser cache. Running
# as root would work and would also mean every file the container writes into a
# mounted volume lands as root on the host.
RUN mkdir -p /data && chown -R pwuser:pwuser /data /app
USER pwuser

VOLUME ["/data"]
EXPOSE 8787

# Inside a container 0.0.0.0 is the only address that is reachable at all, and
# the engine refuses that without authentication - deliberately. Set
# NUMERA_API_TOKEN (see docker-compose.yml) or, if something in front of this
# already authenticates, NUMERA_ALLOW_INSECURE_BIND=1.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD python -c "import os,urllib.request as u; \
t=os.environ.get('NUMERA_API_TOKEN'); \
r=u.Request('http://127.0.0.1:8787/api/health'); \
t and r.add_header('Authorization','Bearer '+t); \
u.urlopen(r,timeout=4).read()" || exit 1

ENTRYPOINT ["numera", "--data-dir", "/data", "serve", "--host", "0.0.0.0", "--port", "8787"]
