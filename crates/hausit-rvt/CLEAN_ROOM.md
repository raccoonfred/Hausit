# Clean-room log for the RVT reader

This crate reads Autodesk Revit files for interoperability. All knowledge of
the format must come from files we created ourselves, from public
documentation, or from published open-source work, never from decompiling
Autodesk software. See CONTRIBUTING.md and ROADMAP.md (sections 4.4 and 5.3).

Add an entry for each finding: the date, what was learned, and how
(which self-made sample files, which public source).

| Date | Finding | Source / method |
|------|---------|-----------------|
| 2026-10-29 | `Formats/Latest` is one gzip member; `Global/*` streams are an 8-byte header and one gzip member; `Partitions/*` are a header and many gzip members back to back. Some members have no gzip trailer. | Scanning the byte streams of a public sample project (`LittleHouse.rvt`, Revit 2022, from the public ADN-DevTech/RevitTrainingMaterial repository) for the gzip signature and inflating with an open-source zlib. |
| 2026-10-29 | Inflated `Formats/Latest` lists classes as a 16-bit length and an ASCII name followed by a 16-bit tag; with the tag's high bit set, a 16-bit zero and the parent class's record follow. Field names are a 32-bit length and an ASCII name (`m_...`). Example hierarchy: `ArcWall` → `VWall` → `HostObj`; `BasicWallType` → `WallType`. Field type codes are not understood yet. | Reading the inflated bytes of the same sample file. |
| 2026-10-26 | `.rvt/.rfa` files are OLE compound files (CFB). `BasicFileInfo` holds the Revit version and build as UTF-16 text. `RevitPreview4.0` holds a PNG preview after a header. | The public Microsoft CFB specification ([MS-CFB]) and listing the streams of self-made sample files with an open-source CFB library. |
