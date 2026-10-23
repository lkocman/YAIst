# YAIst

A local, AI-assisted successor to YaST for openSUSE. Manage the system by
asking in plain language, by typing `/commands`, or in classic panels. All
three run the same actions, and every change waits for your confirmation.

- Runs on any openSUSE: Leap, Tumbleweed, Slowroll and the immutable variants
  (updates use `zypper up`, `zypper dup` or `transactional-update` as fits).
- The model runs locally (llama.cpp); nothing leaves the machine.
- Terminal UI and desktop GUI with the same layout.

Status: **prototype**. See [PLAN.md](PLAN.md) for the design and
[docs/LICENSING.md](docs/LICENSING.md) for licensing.

## Modules

System (hostname, time zone, NTP), Users, Services (systemd), Firewall
(firewalld), SELinux, Packages and Repositories (zypper, OBS community
repositories), Snapshots (snapper: compare, create, roll back) and Upgrade
(Tumbleweed/Slowroll: `zypper dup`; Leap: move to a newer release with the
repositories switched to it, like opensuse-migration-tool).

## Running

```sh
# 1. The local model (rootless podman, Vulkan GPU if available; first start
#    downloads ~2.5 GB)
scripts/llm-server.sh

# 2. Build (needs cargo; without it, builds in a Leap 16.0 container)
cargo build                      # or: scripts/build-in-container.sh build

# 3. Use it
target/debug/yaist-gui           # desktop
target/debug/yaist               # terminal (ctrl-q quits)
```

Command line:

```sh
yaist run /firewall overview             # any read action
yaist run /firewall open-port 8080 --yes # changes need --yes
yaist ask "is ssh running?"              # one request through the agent; plans are never applied
yaist help [module]                      # all /commands
yaist tools                              # the tool definitions the model sees
```

Changes run with your privileges through polkit: systemd, firewalld and
timedated ask for authentication themselves; package, user and SELinux changes
use `pkexec`, and snapshots and Leap upgrades go through `yaist-helper`, a
small privileged program that re-validates its arguments. Plain pkexec asks
for the password every time; `scripts/dev-install-helper.sh` installs the
helper with a polkit policy that keeps the authorisation for a few minutes.
In the planned design all of this moves into a privileged `yaistd` daemon.

## Configuration

| Variable | Default |
|---|---|
| `YAIST_LLM_URL` | `http://127.0.0.1:8080` |
| `YAIST_MODEL` (llm-server.sh) | `unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M` |
| `YAIST_LLM_PORT` (llm-server.sh) | `8080` |

## Layout

```
crates/yaist-core      action model: specs, validation, /command syntax, registry
crates/yaist-modules   system, users, services, firewall, selinux, packages, repos
crates/yaist-agent     llama-server client and tool-calling agent
crates/yaist-ui        shared client model (state, plans, forms, palette)
apps/yaist             TUI + CLI
apps/yaist-gui         egui desktop app
```

## License

Apache-2.0. Bundled fonts are OFL-1.1. Details in
[docs/LICENSING.md](docs/LICENSING.md), [NOTICE](NOTICE) and
[REUSE.toml](REUSE.toml).
