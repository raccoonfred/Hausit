# Clean-room log for the RVT reader

This crate reads Autodesk Revit files for interoperability. All knowledge of
the format must come from files we created ourselves, from public
documentation, or from published open-source work, never from decompiling
Autodesk software. See CONTRIBUTING.md and ROADMAP.md (sections 4.4 and 5.3).

Add an entry for each finding: the date, what was learned, and how
(which self-made sample files, which public source).

| Date | Finding | Source / method |
|------|---------|-----------------|
| 2026-10-26 | `.rvt/.rfa` files are OLE compound files (CFB). `BasicFileInfo` holds the Revit version and build as UTF-16 text. `RevitPreview4.0` holds a PNG preview after a header. | The public Microsoft CFB specification ([MS-CFB]) and listing the streams of self-made sample files with an open-source CFB library. |
