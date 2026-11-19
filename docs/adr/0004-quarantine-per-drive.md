# ADR 0004: A quarantine on each drive

Status: accepted, 20 Nov 2026

## Decision

A folder on a local drive other than the one holding the main quarantine is
moved, by rename, into `<drive>/.agentclean-quarantine/runs/<run id>/items/`
on its own drive. The run's manifest stays in the main store and records that
quarantine as the entry's `volume`, so `history`, `restore` and `purge` work
as before; an entry whose drive is not connected stays in quarantine with a
note.

Network drives (SMB, NFS, AFP, WebDAV and the like, or any macOS mount not
marked `local`) are skipped, as is any drive whose type the mount table does
not show.

Every folder of a drive quarantine must be a real folder (not a link), owned
by the owner of the main store, and not writable by group or others. It is
created that way (`0700`) and checked again before every clean, restore and
purge (invariant I19). Each run's folder there must be new. The top of the
drive may be writable by others only with the sticky bit set, so no one else
can rename the quarantine away between the check and the move. Restore and
purge check again that the quarantine is at the top of a local drive.

## Why

- A rename keeps the one deletion path of ADR 0002: no copying, no second
  way of removing files, the same `reverify` right before the move, and the
  same restore and purge.
- The system Trash was rejected. macOS offers no API to put an item back, so
  restore would break; reaching the Trash API needs Objective-C calls the
  engine's `unsafe` ban rules out, or AppleScript with an Automation prompt;
  and emptying the Trash would delete items behind the manifest's back.
- A rename on a network drive may not be atomic, and its owners and
  permissions can be whatever the server says.
- A shared or second-hand drive may already hold a `.agentclean-quarantine`
  planted to redirect moves or purges; I19 refuses it.

## Consequences

- macOS mounts most external drives with ownership ignored (`noowners`):
  every file appears to belong to whoever looks. On such a drive the owner
  check proves nothing; what still holds is that no folder on the way may be
  a link, each run's folder is new, purge deletes only real folders, and
  restore puts items back only inside their project. Anyone holding the drive
  could change its files anyway.

- A drive that does not keep Unix permissions (exFAT, FAT) may report every
  folder as writable by others; its folders are then skipped with that
  reason.
- Space on another drive comes back only when the run is purged, as on the
  main drive.
- The proposal's list of drive quarantines in the main store was not needed:
  the manifest's `volume` field records where each item went.
