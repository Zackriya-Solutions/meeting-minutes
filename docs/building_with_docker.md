# Building Meetily with Docker

This guide builds Meetily's Linux desktop bundles inside a container, so the host machine does not need Rust, Node.js, WebKitGTK or the other native build dependencies. Docker is the only requirement.

Use it when you want a reproducible build, or when a local build fails on native dependencies such as OpenSSL or WebKitGTK.

## Which bundle to use

The build runs on any Linux host with Docker, but the image is based on Ubuntu 22.04, and that determines where the output runs:

| Bundle | Runs on |
|---|---|
| `.deb` | Debian/Ubuntu and derivatives (Linux Mint, Pop!_OS, ...) with glibc 2.35 or newer, e.g. Ubuntu 22.04+, Debian 12+ |
| AppImage | Most x86_64 distributions with glibc 2.35 or newer, including Fedora and Arch |

On Fedora, Arch and other non-Debian distributions, build the AppImage (`BUNDLES=appimage`), or follow [Building on Linux](building_in_linux.md) for a native build. The Dockerfile does not produce `.rpm` or Arch packages.

## 1. Build the image

The build context must be the repository root. The Cargo workspace is declared in the root `Cargo.toml`, and the build output therefore lands in the root `target/` directory, not `frontend/src-tauri/target/`.

```bash
git clone https://github.com/Zackriya-Solutions/meeting-minutes
cd meeting-minutes

docker build -f frontend/Dockerfile.build -t meetily-builder .
```

The default produces a `.deb`, which only installs on Debian/Ubuntu-based systems. To build the portable AppImage instead of, or in addition to, the `.deb`, pass `BUNDLES`:

```bash
# AppImage only (non-Debian distributions)
docker build -f frontend/Dockerfile.build -t meetily-builder \
  --build-arg BUNDLES=appimage .

# Both
docker build -f frontend/Dockerfile.build -t meetily-builder \
  --build-arg BUNDLES=deb,appimage .
```

## 2. Copy the bundles to the host

```bash
docker create --name meetily-artifacts meetily-builder
docker cp meetily-artifacts:/src/target/release/bundle ./meetily-bundles
docker rm meetily-artifacts
```

The copied directory looks like this:

```
meetily-bundles/deb/meetily_<version>_amd64.deb
meetily-bundles/appimage/meetily_<version>_amd64.AppImage
```

If the bundle directory is missing, the build did not reach the bundling step. Inspect the image:

```bash
docker run --rm meetily-builder ls -la /src/target/release/bundle
```

## 3. Install or run on the host

AppImage (any distribution with glibc 2.35+; needs FUSE 2, e.g. `libfuse2` on Ubuntu, or run it with `--appimage-extract-and-run`):

```bash
chmod +x meetily-bundles/appimage/meetily_*_amd64.AppImage
./meetily-bundles/appimage/meetily_*_amd64.AppImage
```

Deb (Debian/Ubuntu and derivatives only):

```bash
sudo apt install ./meetily-bundles/deb/meetily_*_amd64.deb
```

## The `.deb` and the distro `ffmpeg` package

`bundle.externalBin` in `frontend/src-tauri/tauri.conf.json` includes `binaries/ffmpeg`, so the generated package ships its own `/usr/bin/ffmpeg`. On a system where the `ffmpeg` package is already installed, `dpkg` refuses to overwrite the existing file:

```
trying to overwrite '/usr/bin/ffmpeg', which is also in package ffmpeg
```

Meetily looks for ffmpeg in two places (see `frontend/src-tauri/src/audio/ffmpeg.rs`): a copy next to the installed executable, then `ffmpeg` on `PATH`. The distro build of ffmpeg works fine, so the bundled copy is optional. Build the image with `SYSTEM_FFMPEG=1` to leave it out and declare a dependency on `ffmpeg` instead:

```bash
docker build -f frontend/Dockerfile.build -t meetily-builder \
  --build-arg SYSTEM_FFMPEG=1 .
```

This only applies to the `.deb`, so it only matters on Debian/Ubuntu-based systems. The resulting `.deb` installs alongside the distro `ffmpeg` package. Note that this also drops the bundled ffmpeg from an AppImage built in the same run, so build the AppImage in a separate invocation if you want it to stay self-contained.

Alternatively, run an already-built package without installing it:

```bash
mkdir -p ~/meetily-deb
dpkg-deb -x meetily-bundles/deb/meetily_*_amd64.deb ~/meetily-deb
~/meetily-deb/usr/bin/meetily
```

## Notes and limitations

- The image uses Ubuntu 22.04, so both bundles require glibc 2.35 or newer on the host. Older releases such as Ubuntu 20.04 or Debian 11 cannot run them.
- The `.deb` declares Ubuntu 22.04 package names as dependencies and is meant for Debian/Ubuntu-based systems only. Use the AppImage elsewhere.
- The GUI cannot run inside the container: audio, display and WebKit require host access. Build in Docker, then run the package on the host.
- The container runs as root, so files copied out with `docker cp` are owned by root. Adjust with `sudo chown -R "$USER:$USER" ./meetily-bundles`.
- The first build compiles Whisper and the Rust dependencies and can take 10 minutes or more. Docker layer caching speeds up later builds.
- A `.dockerignore` at the repository root keeps `target/`, `node_modules/` and other build output out of the build context.

## Troubleshooting

### The build fails while compiling `openssl-sys`

The image installs `pkg-config` and `libssl-dev`, which that crate needs. Keep both if you adapt the Dockerfile.

### The build ends with exit code 1 after "A public key has been found, but no private key"

The bundles have already been written at that point. This is the optional Tauri updater signature step; the Dockerfile disables it with `createUpdaterArtifacts: false`. If you build outside Docker, pass the same flag or set `TAURI_SIGNING_PRIVATE_KEY`.

### No bundles after the build

Check the path. The output is `/src/target/release/bundle` inside the container, not `/src/frontend/src-tauri/target/release/bundle`.
