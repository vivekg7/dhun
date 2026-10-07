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
# Never root, even if the compose file forgets `user:` (nobody:nogroup).
USER 65534:65534
EXPOSE 8585
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s CMD ["/dhun", "healthcheck"]
ENTRYPOINT ["/dhun"]
