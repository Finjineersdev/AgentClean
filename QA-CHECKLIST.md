# AgentClean QA checklist

Always QA against throwaway projects with `HOME` pointed at a temporary
folder, so the engine uses a throwaway config and quarantine.

## Automated walkthrough (Debug builds)

```bash
Q=$(mktemp -d); mkdir -p $Q/home $Q/work
# make a few projects in $Q/work: one old Next.js app (package.json + .next +
# node_modules), one old Rust crate (Cargo.toml + target), one recently
# edited project, one pinned with: HOME=$Q/home agentclean pin $Q/work/<name>
HOME=$Q/home AGENTCLEAN_QA=$Q/out \
  build/dd/Build/Products/Debug/AgentClean.app/Contents/MacOS/AgentClean \
  -scanRoots "(\"$Q/work\")"
cat $Q/out/report.txt      # every line must start with PASS
open $Q/out                # window snapshots of every screen
```

The walkthrough expects projects named `shop-web` (dormant Next.js),
`live-api` (active) and `keep-me` (pinned); see `AgentClean/App/QAWalkthrough.swift`.

## UI tests

`xcodebuild test -project AgentClean.xcodeproj -scheme AgentClean -destination 'platform=macOS'`.
On a developer Mac, enable automation mode once:
`sudo automationmodetool enable-automationmode-without-authentication`.
CI runs them on macOS 15 with Xcode 16.

## By hand

| Step | Do | Expect |
| --- | --- | --- |
| 1 | Settings: add your scan folders; Overview: **Scan** | Projects appear while scanning; then Ready, Caution and active totals |
| 2 | Click **Stop** during a scan | Scan stops within a second |
| 3 | Projects: open an idle project | Safe folders ticked, Caution folders not; each row says why it is safe or kept |
| 4 | Open an active project | Nothing ticked; Review stays disabled until "Clean anyway" is on |
| 5 | Review… | The dry run lists exactly the ticked folders; nothing has moved |
| 6 | Move | Folders gone from the project; Undo brings them back |
| 7 | Pin a project | Its folders cannot be ticked; it appears under Settings → Pinned |
| 8 | History: Purge… | A confirmation shows the bytes; cancel keeps everything |
| 9 | Dev Stack | Lists tools; copy a removal command; nothing runs |
