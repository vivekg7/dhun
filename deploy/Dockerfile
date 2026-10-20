# Dhun server image. A static Go binary (no cgo: pure-Go SQLite and taglib)
# on scratch, so the image is the binary and nothing else.
# Build from the repo root: docker build -f deploy/Dockerfile .

FROM --platform=$BUILDPLATFORM golang:1.27-alpine AS build
ARG TARGETOS TARGETARCH VERSION=dev
WORKDIR /src/server
COPY server/go.mod server/go.sum ./
RUN go mod download
COPY server/ ./
RUN CGO_ENABLED=0 GOOS=$TARGETOS GOARCH=$TARGETARCH \
    go build -trimpath -ldflags "-s -w -X main.version=$VERSION" -o /dhun ./cmd/dhun

FROM scratch
COPY --from=build /dhun /dhun
ENV DHUN_MEDIA=/media DHUN_DATA=/data DHUN_ADDR=:8585
# The target is 256 MB at 100,000 songs (docs/REQUIREMENTS.md). A soft limit
# makes the Go collector return memory as it nears that, instead of keeping
# twice the live heap; the compose file's mem_limit is only the kill switch.
ENV GOMEMLIMIT=192MiB
# Never root, even if the compose file forgets `user:` (nobody:nogroup).
USER 65534:65534
EXPOSE 8585
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s CMD ["/dhun", "healthcheck"]
ENTRYPOINT ["/dhun"]
