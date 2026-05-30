FROM node:22-bookworm AS build

ARG EXCALIDRAW_MCP_REF=157aa23ceb1976008aadc89eb05e3444060f09d6

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates git \
  && rm -rf /var/lib/apt/lists/*

RUN corepack enable && corepack prepare pnpm@10.11.0 --activate

WORKDIR /src

RUN git init \
  && git remote add origin https://github.com/excalidraw/excalidraw-mcp.git \
  && git fetch --depth 1 origin "${EXCALIDRAW_MCP_REF}" \
  && git checkout FETCH_HEAD

RUN pnpm install --frozen-lockfile
RUN pnpm run build

FROM node:22-bookworm-slim AS runtime

ENV NODE_ENV=production
ENV PORT=3001

WORKDIR /app

COPY --from=build /src/dist ./dist

EXPOSE 3001

CMD ["node", "dist/index.js"]

