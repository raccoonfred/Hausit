# Installing Hausit on Windows

This guide takes you from a clean Windows 10 or 11 PC to a running copy of
Hausit. It takes about 20–40 minutes, most of it waiting for downloads and the
first build.

Hausit does not have an installer yet, so you build it from the source code.
You only do the setup once. After that, updating is two commands (see
[Updating](#updating)).

## What you need

| Item | Why | Size |
|------|-----|------|
| Windows 10 (1809 or newer) or Windows 11, 64-bit | | |
| Microsoft C++ Build Tools (MSVC + Windows SDK) | Rust uses Microsoft's linker on Windows | ~6 GB |
| Rust 1.95 or newer (installed with `rustup`) | Compiles Hausit | ~1.5 GB |
| Git for Windows | Downloads the source code and updates | ~300 MB |
| About 5 GB free disk space for the build | The `target` folder | |
| A graphics driver with DirectX 12, Vulkan or OpenGL 3.3 | Draws the views | |

Any reasonably recent Intel, AMD or NVIDIA graphics works. Hausit uses
DirectX 12 or Vulkan when it can and falls back to OpenGL automatically.

## Quick version

If you are comfortable with a terminal, open **PowerShell** and run:

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --id Rustlang.Rustup
winget install --id Git.Git
```

Close PowerShell, open a new one (so it picks up the new programs), then:

```powershell
git clone https://github.com/raccoonfred/Hausit.git
cd Hausit
cargo run --release -- --sample
```

The rest of this guide explains each step and what to do if one fails.

## Step 1: Install the Microsoft C++ Build Tools

Rust on Windows needs Microsoft's C++ linker and the Windows SDK.

**Option A, with winget (Windows 10/11 with App Installer):** open PowerShell
and run

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

A Visual Studio Installer window appears and installs everything on its own.

**Option B, by hand:**

1. Download **Build Tools for Visual Studio** from
   <https://visualstudio.microsoft.com/visual-cpp-build-tools/>.
2. Run it. In the **Workloads** tab, tick **Desktop development with C++**.
3. On the right, make sure **MSVC … C++ x64/x86 build tools** and a
   **Windows 11 SDK** (or Windows 10 SDK) are ticked. They are by default.
4. Click **Install** and wait for it to finish.

If you already have Visual Studio 2019 or 2022 (Community, Professional or
Enterprise) with the *Desktop development with C++* workload, you can skip
this step.

## Step 2: Install Rust

**With winget:**

```powershell
winget install --id Rustlang.Rustup
```

**By hand:** download `rustup-init.exe` from <https://rustup.rs>, run it and
press **Enter** to accept the default installation (`x86_64-pc-windows-msvc`,
stable toolchain).

If rustup says it cannot find the Visual Studio C++ build tools, finish
Step 1 first, then run `rustup-init.exe` again.

**Close all PowerShell windows and open a new one**, then check:

```powershell
rustc --version
cargo --version
```

`rustc` must report **1.95.0 or newer**. If it is older, run
`rustup update stable`.

## Step 3: Install Git

**With winget:**

```powershell
winget install --id Git.Git
```

**By hand:** download from <https://git-scm.com/download/win> and run the
installer with its default options.

Open a new PowerShell and check with `git --version`.

> Prefer not to use Git? On the GitHub page click **Code ▸ Download ZIP**,
> extract it, and open PowerShell in the extracted `Hausit-main` folder. You
> will then need to download a new ZIP each time you update.

## Step 4: Download Hausit

Pick a folder for the source code, for example your user folder, and clone
the repository:

```powershell
cd $HOME
git clone https://github.com/raccoonfred/Hausit.git
cd Hausit
```

Avoid folders synced by OneDrive (such as `Documents` or `Desktop` when
OneDrive backup is on). The build writes thousands of files and OneDrive
slows it down and can lock files mid-build.

## Step 5: Build and run

From the `Hausit` folder:

```powershell
cargo run --release
```

The first build downloads Hausit's libraries and compiles everything. Expect
5–15 minutes depending on your PC. Later builds only recompile what changed
and take seconds to a minute.

When it finishes, the Hausit window opens. To start with a sample project:

```powershell
cargo run --release -- --sample              # a furnished house
cargo run --release -- --sample-industrial   # an industrial shed / warehouse
```

Note the `--` before the Hausit options: everything after it is passed to
Hausit instead of to Cargo.

## Step 6: Run Hausit without the terminal (optional)

The build produces a normal Windows program at

```
Hausit\target\release\hausit.exe
```

You can double-click it, or make a shortcut:

1. Open `target\release` in File Explorer.
2. Right-click `hausit.exe` ▸ **Show more options** ▸ **Send to** ▸
   **Desktop (create shortcut)**.
3. Optionally right-click the shortcut ▸ **Pin to Start**.

The `.exe` is self-contained, so you can also copy it anywhere you like, for
example to `C:\Program Files\Hausit\`. Remember to copy it again after you
update.

You can open a file straight away by passing it on the command line or by
dragging it onto `hausit.exe`:

```powershell
.\target\release\hausit.exe "C:\Projects\My House.hausit"
.\target\release\hausit.exe model.ifc
```

## Updating

From the `Hausit` folder:

```powershell
git pull
cargo run --release
```

If `git pull` complains about local changes you did not mean to make, run
`git checkout .` first (this throws away edits to the source code, not your
projects).

Now and then also update Rust itself, since Hausit tracks recent Rust
versions:

```powershell
rustup update stable
```

## Command-line options

```
hausit [FILE] [--sample | --sample-industrial] [--view "View Name"]
       [--export-pdf OUT.pdf] [--export-ifc OUT.ifc] [--export-dxf OUT.dxf]
```

| Option | What it does |
|--------|--------------|
| `FILE` | Opens a `.hausit`, `.ifc`, `.rvt` or `.rfa` file |
| `--sample` / `--sample-industrial` | Opens a sample house or industrial building |
| `--view "Level 1"` | Opens the named view |
| `--export-pdf OUT.pdf` | Writes every sheet to a PDF and exits without a window |
| `--export-ifc OUT.ifc` | Writes the model as IFC4 and exits |
| `--export-dxf OUT.dxf` | Writes the view from `--view` (or the first floor plan) as DXF and exits |

Example:

```powershell
cargo run --release -- --sample --export-pdf house.pdf
```

## Troubleshooting

**`error: linker 'link.exe' not found`**
The C++ Build Tools are missing or incomplete. Redo Step 1 and make sure
*Desktop development with C++* is ticked, then open a new PowerShell.

**`cargo` or `git` "is not recognized as the name of a cmdlet"**
The terminal was opened before the install. Close every PowerShell or
Terminal window and open a new one. If it still fails, sign out of Windows
and back in.

**`package hausit-app ... requires rustc 1.95` (or similar)**
Your Rust is too old. Run `rustup update stable`.

**`failed to run custom build command` mentioning the Windows SDK or
`kernel32.lib`**
The Windows SDK is missing. Open **Visual Studio Installer** ▸ **Modify** on
Build Tools ▸ **Individual components** and tick a *Windows 11 SDK* or
*Windows 10 SDK*.

**The build is very slow or fails with "Access is denied" / "being used by
another process"**
Usually antivirus or OneDrive scanning the `target` folder. Move the
`Hausit` folder out of OneDrive, or add it as an exclusion in **Windows
Security ▸ Virus & threat protection ▸ Manage settings ▸ Exclusions**. Then
run `cargo build --release` again; it continues where it stopped.

**`hausit.exe` does nothing when double-clicked, or the window is black or
blank**
Update your graphics driver from Intel, AMD or NVIDIA (Windows Update's
driver is often old). Hausit falls back to OpenGL when DirectX 12 and Vulkan
are not available, but it needs at least OpenGL 3.3. Inside virtual machines
and Remote Desktop sessions, 3D support may be missing; try running it on the
PC directly. To see the error message, run it from PowerShell with
`cargo run` (without `--release`), which keeps the console output.

**Windows SmartScreen says "Windows protected your PC"**
This can happen for an `.exe` you copied from another PC. Click **More
info** ▸ **Run anyway**. A program you built yourself normally does not
trigger it.

**Out of disk space during the build**
The `target` folder can grow to several gigabytes. Run `cargo clean` in the
`Hausit` folder to delete it; the next build recreates it.

**Something else**
Open an issue at <https://github.com/raccoonfred/Hausit/issues> with your
Windows version, the output of `rustc --version`, and the full error text.

## Uninstalling

1. Delete the `Hausit` folder (and any copy of `hausit.exe` or shortcuts you
   made).
2. Optionally remove the tools: `rustup self uninstall` removes Rust, and
   Git and *Build Tools for Visual Studio* can be removed from **Settings ▸
   Apps ▸ Installed apps**.

Your projects are ordinary `.hausit` files wherever you saved them and are
not touched by uninstalling.
