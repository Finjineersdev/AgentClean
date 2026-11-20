# Third-party assets

## Language and tool logos

`AgentClean/Assets.xcassets/Logos/` holds logos from
[Simple Icons](https://simpleicons.org) 13.21.0, released under
[CC0 1.0](https://github.com/simple-icons/simple-icons/blob/13.21.0/LICENSE.md).
The marks belong to their projects (Node.js, Rust, Python, Go, Swift, Xcode,
Flutter, Dart, PHP, Composer, OpenJDK, Kotlin, Gradle, Android, .NET, npm,
pnpm, Yarn, Bun, PyPI, uv, Deno, CocoaPods, Homebrew, Apache Maven, Docker,
Hugging Face, Ollama, Cypress, JetBrains, PyTorch).
AgentClean shows them only to say which tool a folder belongs to; it is not
endorsed by any of these projects.

To add one: download `icons/<slug>.svg` from the same pinned version, check it
is a single `<path>` with no scripts or links, and add a template imageset
named `logo-<slug>`. Colours live in `AgentClean/Design/Ecosystem.swift`;
brand colours too dark for the graphite UI are lightened there.

## Characters

`AgentClean/Assets.xcassets/Peeps/` holds characters from
[Open Peeps](https://www.openpeeps.com) by Pablo Stanley, released under
[CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/), generated with
the DiceBear `open-peeps` style (9.x) and saved as static SVGs. The app makes
no network request for them. The doodles in `AgentClean/Design/Motion.swift`
are drawn in code.
