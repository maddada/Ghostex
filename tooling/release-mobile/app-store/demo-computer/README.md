# App Store demo computers

Two local Linux containers supply fictional Ghostex content to the iOS simulator. `ghostex-appstore-demo` is shown as **Work computer** and contains Atlas API, Side project, and Wander travel. `ghostex-appstore-home` is shown as **Home computer** and contains Side project. Both use the Ghostex 10.26.0 Linux arm64 release with a local SSH server. The Wander travel page runs on port 3000 inside the work container.

## Start or rebuild

On this Mac, start Colima, then run:

```sh
colima start
GHOSTEX_DEMO_PASSWORD='<local demo password>' sh tooling/release-mobile/app-store/demo-computer/setup.sh
```

The script downloads the pinned `gxserver-linux-arm64.tar.gz` release asset when the image is missing, builds the image, starts both containers, initializes the fictional projects, and registers them with gxserver. Its runtime download is ignored by Git. SSH is exposed only on the Mac's loopback address: port 2222 for Work computer and 2223 for Home computer. Use `docker stop ghostex-appstore-demo ghostex-appstore-home` to leave them stopped without losing the demo sessions.

To reproduce the agent conversations in fresh containers, temporarily copy a local Codex login into each container, run the seed script once, wait for the agents to finish, and remove both copies:

```sh
for machine in ghostex-appstore-demo ghostex-appstore-home; do
  docker exec "$machine" mkdir -p /home/demo/.codex
  docker cp "$HOME/.codex/auth.json" "$machine":/home/demo/.codex/auth.json
  docker exec "$machine" chown -R demo:demo /home/demo/.codex
  docker exec "$machine" chmod 700 /home/demo/.codex
  docker exec "$machine" chmod 600 /home/demo/.codex/auth.json
done
sh tooling/release-mobile/app-store/demo-computer/seed-sessions.sh
for machine in ghostex-appstore-demo ghostex-appstore-home; do
  docker exec "$machine" rm -f /home/demo/.codex/auth.json
done
```

The seed script gives the demo agents only fictional projects. It deliberately creates a pending destination-card question for the approval screenshot. The login is never baked into the image. Remove it after the turns finish; keep the container stopped afterward to preserve the transcripts.

## Connect the simulator

In Ghostex on the simulator, choose **Connect a computer**, then the manual **Or type the details** link. The form is labeled **Connect over Tailscale**, but it accepts a plain address. Enter `Work computer`, address `127.0.0.1`, username `demo`, and the local demo password. Open **Advanced**, set **SSH port** to `2222`, and connect. Add a second machine with name `Home computer` and port `2223`. Use the same plain address and username. The Tailscale precheck is informational for this local simulator setup.

These loopback ports work only on this Mac's simulators. App Review uses the in-app demo described in `../README.md` (Try a demo computer). Keep this SSH login on this computer.
