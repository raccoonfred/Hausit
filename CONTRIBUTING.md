# Contributing to Hausit

Thanks for helping. Hausit follows the workflow of Autodesk Revit so that
architects and engineers can switch without relearning, but everything in
this repository must be our own work. Please read the rules below before
opening a pull request. [ROADMAP.md](ROADMAP.md) explains what is planned
and why these rules exist (section 5).

## Clean-room rules

- **No Autodesk artwork.** Do not copy, trace or closely imitate Revit's
  icons, splash screens, cursors or other graphics. Hausit's icons are drawn
  from scratch as vector shapes in `crates/hausit-app/src/icons.rs`.
- **No Autodesk text.** Do not paste Revit help pages, tooltips or dialog
  wording. Short functional labels such as "Base Offset" or "Attach
  Top/Base" are fine. Write descriptions in your own words.
- **No Autodesk content.** Do not add Revit families (`.rfa`), templates
  (`.rte`), title blocks, materials or sample projects, or anything derived
  from them. Hausit's library is authored from scratch. Data from published
  standards (for example steel section tables) is fine; cite the standard.
- **No decompiled code.** Never decompile or disassemble Revit or other
  Autodesk software, and do not contribute code or notes derived from doing
  so. Work on the `.rvt` reader (`crates/hausit-rvt`) only from files you
  created yourself and from public documentation, and record what you
  learned and how in the clean-room log in that crate.
- **Trademarks.** Do not use "Revit" or "Autodesk" in product names, logos,
  app icons or window titles. Describing compatibility in plain text ("opens
  Revit files") is fine, together with the notice in the README.

## Code

- Rust stable, formatted with `cargo fmt --all`.
- `cargo clippy --workspace --all-targets -- -D warnings` must pass.
- `cargo test --workspace` must pass. Add tests for model and geometry
  changes.
- Internal units are millimetres.
- Match the surrounding code: naming, comments and structure.

## Licence

By contributing you agree that your contribution is licensed under the
MIT license or the Apache License 2.0, at the user's option, like the rest of
Hausit.
