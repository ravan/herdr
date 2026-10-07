# Publish Herdr Houston

**Houston, we have a pane**

Herdr Houston is Ravan's fork with collections, missions, and Mission control.
Its executable is `herdr-houston` (`herdr-houston.exe` on Windows).

## Publish a build

1. Commit and push the source and the Houston workflow to `ravan/herdr`.
2. Read the package version in `Cargo.toml`. Create an annotated tag with that
   base version and a positive build number. For package version `0.9.3`:

   ```bash
   git tag -a houston-v0.9.3-1 -m "Houston, we have a pane"
   git push origin houston-v0.9.3-1
   ```

3. Watch the **Herdr Houston** workflow. Both the original tag actor and the
   rerun actor must have repository admin permission.
4. Download the asset for your platform from the GitHub prerelease. Verify its
   SHA-256 against `SHA256SUMS`.

The example tag builds version `0.9.3-houston.1`. Increment the final build
number for another build of the same package version. Existing releases are
never overwritten. A failed upload can leave a draft for manual inspection.
Use a new build number rather than reusing a published tag.

The workflow publishes Linux x86_64/aarch64, macOS x86_64/aarch64, and Windows
x86_64 assets. `FORK_BUILD.json` records the source commit, identity, and hashes.

## Install beside Herdr

On macOS arm64, install the downloaded asset with its own command name:

```bash
mkdir -p "$HOME/.local/bin"
install -m 755 herdr-houston-macos-aarch64 "$HOME/.local/bin/herdr-houston.new"
mv "$HOME/.local/bin/herdr-houston.new" "$HOME/.local/bin/herdr-houston"
herdr-houston --version
```

Choose the matching asset on another Unix platform. On Windows, extract the
whole ZIP into a separate directory. Keep `conpty` and the included notices
beside `herdr-houston.exe`.

Launch from a fresh terminal tab. Release builds use `herdr-houston` config and
state directories. Opt-in debug builds use `herdr-houston-dev`. On Unix the
defaults are `~/.config/herdr-houston` and `~/.local/state/herdr-houston`.
Windows uses the matching directories beneath `%APPDATA%` and `%LOCALAPPDATA%`.
Explicit config and socket overrides still take precedence. Inherited
`HERDR_SOCKET_PATH` and `HERDR_CLIENT_SOCKET_PATH` can target another session.

Houston uses manual GitHub releases. Local self-update, background upstream
update offers, and automatic SSH installation are disabled in Houston builds.
For SSH, install a compatible `herdr-houston` on the remote machine and add it
to `PATH` before attaching. Agent integration installation still uses the
agent's normal configuration.

## Build locally

```bash
HERDR_BUILD_FORK=houston HERDR_BUILD_ID=1 cargo build --release --locked
./target/release/herdr --version
```

Cargo's output filename remains `herdr`. The workflow packages it under the
Houston name. Without `HERDR_BUILD_FORK`, normal builds retain their existing
namespaces, versions, and update behavior.
