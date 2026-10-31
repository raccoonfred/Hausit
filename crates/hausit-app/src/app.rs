//! Application state and the top-level layout, which follows Revit:
//! quick access toolbar and ribbon on top, options bar below it,
//! Properties and Project Browser on the left, view tabs and the drawing
//! area in the middle, status bar at the bottom.

use crate::dialogs::Dialogs;
use crate::tools::Tool;
use hausit_core::{DVec2, ElementId};
use hausit_model::{template, Category, Document, History, ModelError, ViewKind};
use hausit_views::DisplayList;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RibbonTab {
    Architecture,
    Structure,
    Insert,
    Annotate,
    MassingSite,
    Collaborate,
    View,
    Manage,
    Modify,
}

impl RibbonTab {
    pub const ALL: [RibbonTab; 9] = [
        RibbonTab::Architecture,
        RibbonTab::Structure,
        RibbonTab::Insert,
        RibbonTab::Annotate,
        RibbonTab::MassingSite,
        RibbonTab::Collaborate,
        RibbonTab::View,
        RibbonTab::Manage,
        RibbonTab::Modify,
    ];
    pub fn label(self) -> &'static str {
        match self {
            RibbonTab::Architecture => "Architecture",
            RibbonTab::Structure => "Structure",
            RibbonTab::Insert => "Insert",
            RibbonTab::Annotate => "Annotate",
            RibbonTab::MassingSite => "Massing & Site",
            RibbonTab::Collaborate => "Collaborate",
            RibbonTab::View => "View",
            RibbonTab::Manage => "Manage",
            RibbonTab::Modify => "Modify",
        }
    }
}

/// Pan/zoom of one open view. `center` is the view coordinate shown at the
/// middle of the canvas; `zoom` is pixels per millimetre.
#[derive(Clone, Copy, Debug)]
pub struct Camera2D {
    pub center: DVec2,
    pub zoom: f64,
    pub fitted: bool,
}

impl Default for Camera2D {
    fn default() -> Self {
        Camera2D {
            center: DVec2::ZERO,
            zoom: 0.05,
            fitted: false,
        }
    }
}

/// Display list of the active view, rebuilt when the model or view changes.
#[derive(Default)]
pub struct GraphicsCache {
    key: Option<(ElementId, u64)>,
    pub list: DisplayList,
}

pub struct HausitApp {
    pub doc: Document,
    pub history: History,
    pub path: Option<PathBuf>,
    pub saved_revision: u64,
    pub show_home: bool,

    pub open_views: Vec<ElementId>,
    pub active_view: Option<ElementId>,
    pub cameras: HashMap<ElementId, Camera2D>,
    pub cache: GraphicsCache,

    pub selection: Vec<ElementId>,
    pub hover: Option<ElementId>,
    pub pick_candidates: Vec<ElementId>,
    pub pick_cycle: usize,
    pub tool: Tool,
    pub ribbon_tab: RibbonTab,
    pub status: String,
    /// Type the next placed element of each category will use.
    pub current_type: HashMap<Category, ElementId>,
    pub options: crate::tools::ToolOptions,
    pub dialogs: Dialogs,
    pub shortcut: (String, f64),
    /// Copy to Clipboard: the elements copied (Ctrl+C), pasted with Ctrl+V.
    pub clipboard: Vec<ElementId>,
    /// Edit Group mode: the group whose members are picked one by one,
    /// and the model before the edit (for Cancel).
    pub group_edit: Option<ElementId>,
    pub group_edit_before: Option<Box<Document>>,
    pub title: String,
}

impl Default for HausitApp {
    fn default() -> Self {
        let mut app = HausitApp {
            doc: template::new_project(),
            history: History::default(),
            path: None,
            saved_revision: 0,
            show_home: true,
            open_views: Vec::new(),
            active_view: None,
            cameras: HashMap::new(),
            cache: GraphicsCache::default(),
            selection: Vec::new(),
            hover: None,
            pick_candidates: Vec::new(),
            pick_cycle: 0,
            tool: Tool::Modify,
            ribbon_tab: RibbonTab::Architecture,
            status: String::new(),
            current_type: HashMap::new(),
            options: Default::default(),
            dialogs: Dialogs::default(),
            shortcut: (String::new(), 0.0),
            clipboard: Vec::new(),
            group_edit: None,
            group_edit_before: None,
            title: String::new(),
        };
        app.saved_revision = app.doc.revision();
        app
    }
}

impl HausitApp {
    // ----- documents ----------------------------------------------------------

    /// Replaces the open project.
    pub fn set_document(&mut self, doc: Document, path: Option<PathBuf>) {
        self.doc = doc;
        self.history.clear();
        self.path = path;
        self.saved_revision = self.doc.revision();
        self.open_views.clear();
        self.cameras.clear();
        self.active_view = None;
        self.selection.clear();
        self.tool = Tool::Modify;
        self.current_type.clear();
        self.show_home = false;
        // Open the first floor plan, as Revit opens the last-used view.
        let first = self
            .doc
            .views()
            .find(|v| matches!(v.kind, ViewKind::FloorPlan { .. }))
            .or_else(|| self.doc.views().next())
            .map(|v| v.id);
        if let Some(v) = first {
            self.open_view(v);
        }
        self.status = "Ready".into();
    }

    pub fn is_dirty(&self) -> bool {
        self.doc.revision() != self.saved_revision && !self.history.is_empty()
    }

    pub fn project_title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Project1".into())
    }

    pub fn open_path(&mut self, path: &Path) {
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(hausit_model::component::EXTENSION))
        {
            crate::family_editor::open_family(self, path);
            return;
        }
        let is_ifc = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ifc"));
        if is_ifc {
            match hausit_io::import_ifc(path) {
                Ok((doc, report)) => {
                    self.set_document(doc, None);
                    self.status = format!("{} {}", path.display(), report.summary());
                }
                Err(e) => self.dialogs.error = Some(format!("Could not open {}:\n{e}", path.display())),
            }
            return;
        }
        match hausit_io::open(path) {
            Ok(doc) => {
                let keep = path.extension().map(|e| e == hausit_io::EXTENSION).unwrap_or(false);
                self.set_document(doc, keep.then(|| path.to_path_buf()));
                self.status = format!("Opened {}", path.display());
            }
            Err(hausit_io::IoError::Revit(hausit_rvt::RvtError::ElementDataNotSupported(info))) => {
                self.dialogs.revit_file = Some(crate::dialogs::RevitFileDialog {
                    path: path.to_path_buf(),
                    info: *info,
                });
            }
            Err(e) => self.dialogs.error = Some(format!("Could not open {}:\n{e}", path.display())),
        }
    }

    pub fn save_to(&mut self, path: &Path) -> bool {
        let mut path = path.to_path_buf();
        if path.extension().is_none() {
            path.set_extension(hausit_io::EXTENSION);
        }
        match hausit_io::save(&self.doc, &path) {
            Ok(()) => {
                self.path = Some(path.clone());
                self.saved_revision = self.doc.revision();
                self.status = format!("Saved {}", path.display());
                true
            }
            Err(e) => {
                self.dialogs.error = Some(format!("Could not save:\n{e}"));
                false
            }
        }
    }

    pub fn save(&mut self) -> bool {
        match self.path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    pub fn save_as(&mut self) -> bool {
        let dialog = rfd::FileDialog::new()
            .add_filter("Hausit project", &[hausit_io::EXTENSION])
            .set_file_name(format!("{}.{}", self.project_title(), hausit_io::EXTENSION));
        match dialog.save_file() {
            Some(p) => self.save_to(&p),
            None => false,
        }
    }

    pub fn open_dialog(&mut self) {
        let mut exts = hausit_io::openable_extensions();
        exts.push(hausit_model::component::EXTENSION);
        let picked = rfd::FileDialog::new()
            .add_filter("All supported files", &exts)
            .add_filter("Hausit project", &[hausit_io::EXTENSION])
            .add_filter("IFC", &["ifc"])
            .add_filter("Hausit family", &[hausit_model::component::EXTENSION])
            .add_filter("Autodesk Revit files", &hausit_rvt::EXTENSIONS)
            .pick_file();
        if let Some(p) = picked {
            self.open_path(&p);
        }
    }

    // ----- transactions -------------------------------------------------------

    /// Runs a modifying command as one undoable transaction.
    pub fn transact<T>(&mut self, name: &str, f: impl FnOnce(&mut Document) -> Result<T, ModelError>) -> Option<T> {
        let before = self.doc.clone();
        match f(&mut self.doc) {
            Ok(v) => {
                self.history.commit(name, before);
                self.selection.retain(|id| self.doc.contains(*id));
                self.open_views.retain(|id| self.doc.view(*id).is_some());
                if self.active_view.map(|v| self.doc.view(v).is_none()).unwrap_or(false) {
                    self.active_view = self.open_views.last().copied();
                }
                Some(v)
            }
            Err(e) => {
                self.doc = before;
                self.doc.touch();
                self.dialogs.error = Some(e.to_string());
                None
            }
        }
    }

    pub fn undo(&mut self) {
        self.tool.cancel();
        match self.history.undo(&mut self.doc) {
            Some(n) => self.status = format!("Undid: {n}"),
            None => self.status = "Nothing to undo".into(),
        }
        self.after_history_change();
    }

    pub fn redo(&mut self) {
        self.tool.cancel();
        match self.history.redo(&mut self.doc) {
            Some(n) => self.status = format!("Redid: {n}"),
            None => self.status = "Nothing to redo".into(),
        }
        self.after_history_change();
    }

    fn after_history_change(&mut self) {
        self.selection.retain(|id| self.doc.contains(*id));
        self.open_views.retain(|id| self.doc.view(*id).is_some());
        if self.active_view.map(|v| self.doc.view(v).is_none()).unwrap_or(true) {
            self.active_view = self.open_views.last().copied();
        }
    }

    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let (elements, views): (Vec<ElementId>, Vec<ElementId>) = self.selection.iter().partition(|id| self.doc.element(**id).is_some());
        let n = self.selection.len();
        // On a sheet, Delete takes views off the sheet; the views stay.
        if let Some(sheet) = crate::sheets::active_sheet(self) {
            self.transact("Remove from Sheet", |doc| {
                for v in &views {
                    doc.remove_viewport(sheet, *v);
                }
                Ok(())
            });
            self.selection.clear();
            self.status = format!("Removed {} view(s) from the sheet", views.len());
            return;
        }
        self.transact("Delete", |doc| {
            for v in &views {
                doc.delete_view(*v);
            }
            if !elements.is_empty() {
                doc.check_unpinned(&elements)?;
                doc.delete(&elements)?;
            }
            Ok(())
        });
        self.selection.clear();
        self.status = format!("Deleted {n} element(s)");
    }

    // ----- views --------------------------------------------------------------

    pub fn open_view(&mut self, id: ElementId) {
        if self.doc.view(id).is_none() {
            return;
        }
        if !self.open_views.contains(&id) {
            self.open_views.push(id);
        }
        if self.active_view != Some(id) {
            self.active_view = Some(id);
            self.tool.cancel_keep_kind();
        }
        self.show_home = false;
    }

    pub fn close_view(&mut self, id: ElementId) {
        self.open_views.retain(|v| *v != id);
        if self.active_view == Some(id) {
            self.active_view = self.open_views.last().copied();
        }
    }

    /// The display list of the active view, rebuilt only when needed.
    pub fn active_graphics(&mut self) -> Option<&DisplayList> {
        let id = self.active_view?;
        let view = self.doc.view(id)?;
        let key = (id, self.doc.revision());
        if self.cache.key != Some(key) {
            self.cache.list = hausit_views::build(&self.doc, view);
            self.cache.key = Some(key);
        }
        Some(&self.cache.list)
    }

    /// Level of the active plan view, if the active view is a plan.
    pub fn active_level(&self) -> Option<ElementId> {
        self.doc.view(self.active_view?)?.kind.plan_level()
    }

    /// Type the next placed element of `cat` uses (first type by default).
    pub fn type_for(&mut self, cat: Category) -> Option<ElementId> {
        // Isolated and wall foundations share a category but not types.
        let want = self.tool.wall_foundation();
        let ok = |t: &hausit_model::FamilyType| cat != Category::StructuralFoundations || want.is_none_or(|w| t.is_wall_foundation() == w);
        if let Some(t) = self.current_type.get(&cat) {
            if self.doc.family_type(*t).is_some_and(ok) {
                return Some(*t);
            }
        }
        let t = self.doc.types_of(cat).find(|t| ok(t))?.id;
        self.current_type.insert(cat, t);
        Some(t)
    }

    // ----- frame --------------------------------------------------------------

    fn update_title(&mut self, ctx: &egui::Context) {
        let view = self.active_view.and_then(|v| self.doc.view(v)).map(|v| {
            let kind = match v.kind {
                ViewKind::FloorPlan { .. } => "Floor Plan",
                ViewKind::CeilingPlan { .. } => "Reflected Ceiling Plan",
                ViewKind::ThreeD => "3D View",
                ViewKind::Elevation { .. } => "Elevation",
                ViewKind::Section { .. } => "Section",
                ViewKind::Schedule(_) => "Schedule",
                ViewKind::Sheet(_) => "Sheet",
            };
            format!(" - {kind}: {}", v.name)
        });
        let title = if self.show_home {
            "Hausit - Home".to_string()
        } else {
            format!(
                "Hausit - {}{}{}",
                self.project_title(),
                if self.is_dirty() { "*" } else { "" },
                view.unwrap_or_default()
            )
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.is_dirty() && !self.dialogs.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dialogs.confirm_close = true;
        }
    }
}

impl eframe::App for HausitApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_close_request(&ctx);
        // New elements are created in the active view's phase.
        self.doc.creation_phase = self.active_view.and_then(|v| self.doc.view(v)).and_then(|v| self.doc.view_phase(v));
        crate::tools::handle_shortcuts(self, &ctx);

        crate::ribbon::show(self, ui);
        if !self.show_home {
            crate::options_bar::show(self, ui);
        }
        egui::Panel::bottom("status_bar")
            .exact_size(24.0)
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::TAB_BAR_BG)
                    .inner_margin(egui::Margin::symmetric(8, 3)),
            )
            .show(ui, |ui| self.status_bar(ui));

        if !self.show_home {
            egui::Panel::left("left_panels")
                .resizable(true)
                .default_size(300.0)
                .min_size(220.0)
                .frame(egui::Frame::new().fill(crate::theme::PANEL_BG).inner_margin(egui::Margin::same(0)))
                .show(ui, |ui| {
                    let h = ui.available_height();
                    egui::Panel::top("properties_panel")
                        .resizable(true)
                        .default_size(h * 0.5)
                        .min_size(120.0)
                        .frame(egui::Frame::new().inner_margin(egui::Margin::same(0)))
                        .show(ui, |ui| crate::properties::show(self, ui));
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(egui::Margin::same(0)))
                        .show(ui, |ui| crate::browser::show(self, ui));
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(crate::theme::TAB_BAR_BG))
            .show(ui, |ui| {
                if self.show_home {
                    crate::dialogs::home(self, ui);
                } else {
                    crate::canvas::show(self, ui);
                }
            });

        crate::dialogs::show(self, &ctx);
        self.update_title(&ctx);
    }
}

impl HausitApp {
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(&self.status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{} elements", self.doc.elements().count()));
                ui.separator();
                if !self.doc.option_sets.is_empty() {
                    crate::worksharing::editing_option_combo(self, ui);
                    ui.separator();
                }
                if self.doc.worksharing() {
                    crate::worksharing::active_workset_combo(self, ui, "status_workset");
                    ui.separator();
                }
                if !self.selection.is_empty() {
                    ui.label(format!("Selected: {}", self.selection.len()));
                    ui.separator();
                }
                if let Some(h) = self.hover {
                    ui.label(self.doc.describe(h));
                }
            });
        });
    }
}

// ----- section boxes ------------------------------------------------------------

/// A section box around the whole model, with a little room to spare.
pub fn default_section_box(doc: &Document) -> Option<hausit_model::SectionBox> {
    let b = hausit_model::geometry::model_bounds(doc)?;
    let m = glam::DVec3::splat(500.0);
    Some(hausit_model::SectionBox::around(b.min - m, b.max + m))
}

impl HausitApp {
    /// Opens a 3D view cut to `section_box`, looking from `yaw` (Revit's
    /// "Selection Box" and "Orient to View"). Reuses the 3D view made
    /// for the same purpose.
    pub fn open_3d_with_box(&mut self, name: &str, section_box: hausit_model::SectionBox, yaw: f64) {
        let existing = self
            .doc
            .views()
            .find(|v| v.name == name && matches!(v.kind, ViewKind::ThreeD))
            .map(|v| v.id);
        let name = name.to_string();
        let id = self.transact("Section Box", |doc| {
            let id = existing.unwrap_or_else(|| doc.add_view(&name, ViewKind::ThreeD));
            if let Some(v) = doc.view_mut(id) {
                v.section_box = Some(section_box);
                v.camera.yaw = yaw;
                v.camera.pitch = 20.0;
            }
            Ok(id)
        });
        if let Some(id) = id {
            if let Some(c) = self.cameras.get_mut(&id) {
                c.fitted = false;
            }
            self.open_view(id);
        }
    }

    /// 3D view of a section: the model behind the section line, cut on it.
    pub fn section_in_3d(&mut self, section: ElementId) {
        let Some(v) = self.doc.view(section) else { return };
        let ViewKind::Section { start, end, far_clip } = v.kind else {
            return;
        };
        let name = format!("3D - {}", v.name);
        let Some(model) = hausit_model::geometry::model_bounds(&self.doc) else {
            return;
        };
        let d = (end - start).normalize_or_zero();
        if d == glam::DVec2::ZERO {
            return;
        }
        // Box x runs along the section line, y away from the viewer.
        let look = glam::DVec2::new(d.y, -d.x);
        let angle = d.y.atan2(d.x);
        let mut b = hausit_model::SectionBox {
            min: glam::DVec3::ZERO,
            max: glam::DVec3::ZERO,
            angle,
        };
        let (s, e) = (b.to_local(start.extend(0.0)), b.to_local(end.extend(0.0)));
        b.min = glam::DVec3::new(s.x.min(e.x), s.y - far_clip, model.min.z - 500.0);
        b.max = glam::DVec3::new(s.x.max(e.x), s.y, model.max.z + 500.0);
        // Look the way the section looks, a little from the side.
        let yaw = look.x.atan2(look.y).to_degrees() - 25.0;
        self.open_3d_with_box(&name, b, yaw);
    }

    /// Revit's Selection Box (BX): a 3D view cut to the selected elements.
    pub fn selection_box(&mut self) {
        let mut b = hausit_core::geom::Aabb::EMPTY;
        for g in hausit_model::geometry::model_geometry(&self.doc) {
            if self.selection.contains(&g.id) {
                b = b.union(g.bounds());
            }
        }
        if b.min.x > b.max.x {
            self.status = "Select elements first, then Selection Box (BX).".into();
            return;
        }
        let m = glam::DVec3::splat(300.0);
        let yaw = self
            .doc
            .views()
            .find(|v| matches!(v.kind, ViewKind::ThreeD))
            .map(|v| v.camera.yaw)
            .unwrap_or(-45.0);
        self.open_3d_with_box("3D - Selection Box", hausit_model::SectionBox::around(b.min - m, b.max + m), yaw);
    }
}
