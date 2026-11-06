# Third-party notices

Ghostex includes the third-party software below. Each part keeps its own licence; this file reproduces the notices those licences require for binary distribution.

## Ghostty

The terminal engine (on macOS through GhosttyKit) and its zsh shell integration, vendored in `.dependencies/ghostty/` from <https://github.com/ghostty-org/ghostty> at the commit in `.dependencies/ghostty-patches/UPSTREAM`, MIT:

```
MIT License

Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors

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
```

## iTerm2-Color-Schemes (terminal colour themes)

The terminal colour themes built into the app, from Ghostty's theme package: a release of <https://github.com/mbadolato/iTerm2-Color-Schemes>, pinned as `iterm2_themes` in `.dependencies/ghostty/build.zig.zon`. The MIT licence below covers the collection; as its last paragraph says, the copyright and licence of each individual theme belong to that theme's author.

```
MIT License

Copyright (c) 2011 to Present Mark Badolato

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

This license covers the iTerm-Color-Schemes repository collection of themes.

The copyright/license for each individual theme belongs to the author of that theme.
```

## Sonora (sonora, sonora-aec3, sonora-agc2, sonora-ns, sonora-common-audio, sonora-simd, sonora-fft)

Echo cancellation and noise suppression for the voice agent and Dictate on Windows and Linux. A Rust port of the WebRTC audio processing module. <https://github.com/dignifiedquire/sonora>, BSD-3-Clause:

```
Copyright (c) 2011, The WebRTC Project Authors. All rights reserved.
Copyright (c) 2016, Arun Raghavan and contributors. All rights reserved.
Copyright (c) 2026, dignifiedquire. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

  * Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.

  * Redistributions in binary form must reproduce the above copyright
    notice, this list of conditions and the following disclaimer in
    the documentation and/or other materials provided with the
    distribution.

  * Neither the name of Google nor the names of its contributors may
    be used to endorse or promote products derived from this software
    without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## GBots runtime (OpenMausBot)

GBots, the bots of the GBots extension, run on a copy of OpenMausBot's headless server, <https://github.com/milind-soni/OpenMausBot>, at the commit pinned in `tooling/gbots/upstream.json`, with its `enterprise/` folder (source-available, not Apache-2.0) removed and its product name replaced by GBots (OpenMausBot's name and mascot are trademarks of their owner; GBots is not affiliated with or endorsed by them). It is not part of the app download: it is an on-demand component that the computer GBots runs on downloads when GBots is turned on. Apache License, Version 2.0 (<https://www.apache.org/licenses/LICENSE-2.0>); the component carries the full licence (`LICENSE`), this NOTICE, the licences of the code it adapts (`third_party/`) and a list of the licences of the packages its server bundles (`dependency-licenses.json`). Its NOTICE:

```
OpenMausBot
Copyright 2026 Milind Soni and OpenMausBot contributors

OpenMausBot was relicensed from the MIT License to the Apache License,
Version 2.0, with the consent of all contributors whose copyright was
incorporated before the transition.

The Google Antigravity ACP integration adapts release, installation, profile,
authentication, and runtime temporary-folder behavior from T3 Code, Copyright
2026 T3 Tools Inc., used under the MIT License (third_party/t3-code/LICENSE).

Selected-text citation matching adapts T3 Code's assistant citation selection,
Copyright 2026 T3 Tools Inc., used under the MIT License.

The egress gate for the bots' browser (server/browser/egress/) adapts T3 Code's
public-only SOCKS5 proxy for HTML previews and the Linux listener-ownership
scan of its preview port scanner, Copyright 2026 T3 Tools Inc., used under the
MIT License (third_party/t3-code/LICENSE).

Prompt-cache keep-warm (server/cache-keepwarm.ts and
server/drivers/claude-keepwarm.ts) ports the design of Cache Tax,
https://github.com/karanb192/claude-code-mods, Copyright (c) 2026 Karan
Bansal, used under the MIT License (third_party/cache-tax/LICENSE).

Moving a Grok Bot (shared/grok-import.ts and server/grok-local.ts) adapts the
export message, reader repairs, credential refusal and desktop cache reading of
the agent-importers plugin in hermes-official-plugins,
https://github.com/NousResearch/hermes-official-plugins, Copyright (c) 2026
Nous Research, used under the MIT License: Permission is hereby granted, free
of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without
restriction, including without limitation the rights to use, copy, modify,
merge, publish, distribute, sublicense, and/or sell copies of the Software, and
to permit persons to whom the Software is furnished to do so, subject to the
following conditions: The above copyright notice and this permission notice
shall be included in all copies or substantial portions of the Software. THE
SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

The secure phone credential transport uses @hpke/core and @hpke/common from
hpke-js, Copyright 2023-2024 Ajitomi Daisuke, used under the MIT License.

The Android companion app bundles a prebuilt WebRTC library for Live calls
(io.github.webrtc-sdk:android), built from the WebRTC project, Copyright (c)
2011, The WebRTC project authors, used under the BSD 3-Clause License. WebRTC
includes third-party components under their own licenses, listed with the
library.

Selected provider icon paths are from Simple Icons, used under CC0 1.0.
The Cohere provider icon path is from Lobe Icons, Copyright 2023 LobeHub,
used under the MIT License.

The model provider catalog bundles provider and model data from models.dev,
https://github.com/anomalyco/models.dev, Copyright (c) 2025 models.dev, used
under the MIT License. Its cache and refresh logic is adapted from OpenCode,
https://github.com/anomalyco/opencode, Copyright (c) 2025 opencode, used under
the MIT License. The full license texts are in third_party/models-dev/LICENSE
and third_party/opencode/LICENSE.

The browser driver's US keyboard layout and key-event rules
(server/browser/cdp/keyboard.ts) come from Puppeteer, Copyright 2017 Google
Inc., used under the Apache License, Version 2.0 (third_party/puppeteer).

The browser driver's action semantics, error messages, limits and test cases,
its copy capture and its browser host diagnosis adapt T3 Code's server
browser (apps/server/src/preview), Copyright 2026 T3 Tools Inc., used under
the MIT License (third_party/t3-code/LICENSE).

The bots' browser service on that driver (server/browser/) adapts T3 Code's
server browser and preview tools: its session control, tab lifecycle,
viewers and screencast pacing, tool definitions and snapshot bound
(apps/server/src/preview and apps/server/src/mcp), Copyright 2026 T3 Tools
Inc., used under the MIT License (third_party/t3-code/LICENSE).

The social post previews template (skills/social-posts/posts.html) adapts the
post card markup and platform styling of MiniMock,
https://github.com/minianon/MiniMock, Copyright (c) 2026 Tushar Bhardwaj, used
under the MIT License (third_party/minimock/LICENSE). Its icon paths are from
Lucide, https://lucide.dev, Copyright Lucide Contributors, used under the ISC
License; the template carries both notices, so every copy of it keeps them.

The browser desktop viewer bundles the unmodified noVNC 1.7.0 library,
Copyright the noVNC authors, under MPL-2.0 and its accompanying licenses.
Attribution, license terms and the corresponding source location are in
public/novnc-NOTICE.txt (novnc-NOTICE.txt in the built UI).
The iOS companion app bundles a prebuilt WebRTC library for Live calls
(stasel/WebRTC), built from the WebRTC project, Copyright (c) 2011, The WebRTC
project authors, used under the BSD 3-Clause License. WebRTC includes
third-party components under their own licenses, listed with the library.
```

The component also ships Node.js 24.22.2, <https://nodejs.org>, Copyright Node.js contributors, used under the MIT License; Node's licence, with the notices of the software it includes, is in the component's `node/LICENSE`.
