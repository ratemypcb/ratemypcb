# RateMyPCB KiCad plugin

This milestone-two client uses KiCad's supported IPC API and delegates every
check to the versioned RateMyPCB CLI JSON contract. It never changes or uploads
the active board.

The preview package expects `ratemypcb` on `PATH` or in `bin/`; release packaging
places the matching signed platform binary in that directory. Enable KiCad's API
server under Preferences → Plugins, then install the package through KiCad's
Plugin and Content Manager.

`pcm/metadata.json` is a release template. Replace its checksum and size fields
with generated values before submitting the first package to KiCad's official
addon repository.
