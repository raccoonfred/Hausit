//! Visibility/Graphics (VV) and View Templates: which categories a view
//! shows and how, and the templates that set them for many views at once.

use crate::app::HausitApp;
use egui::{Color32, Context, RichText, Ui};
use hausit_core::ElementId;
use hausit_model::{Category, CategoryGraphics, DetailLevel, Graphics, ViewKind, ViewTemplate, VisualStyle};

/// What a Visibility/Graphics dialog edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VgTarget {
    View(ElementId),
    Template(ElementId),
}

pub struct VgDialog {
    pub target: VgTarget,
    pub rows: Vec<(Category, CategoryGraphics)>,
    pub annotation: bool,
}

/// Categories on the Model tab and the Annotation tab.
fn is_annotation_tab(c: Category) -> bool {
    matches!(
        c,
        Category::Levels | Category::Grids | Category::Dimensions | Category::TextNotes | Category::Tags | Category::RoomSeparation
    )
}

fn rows_of(g: &Graphics) -> Vec<(Category, CategoryGraphics)> {
    let mut cats: Vec<Category> = Category::ALL.to_vec();
    cats.sort_by_key(|c| c.name());
    cats.into_iter().map(|c| (c, g.get(&c).copied().unwrap_or_default())).collect()
}

/// Opens Visibility/Graphics for the active view, or for the template
/// that controls its graphics.
pub fn open_for_active_view(app: &mut HausitApp) {
    let Some(view) = app.active_view.and_then(|v| app.doc.view(v)).cloned() else {
        return;
    };
    if !view.kind.is_graphical() || matches!(view.kind, ViewKind::Sheet(_)) {
        app.status = "Visibility/Graphics applies to plans, sections, elevations and 3D views.".into();
        return;
    }
    if let Some(t) = view
        .template
        .and_then(|t| app.doc.view_template(t))
        .filter(|t| t.graphics.is_some())
    {
        app.status = format!(
            "The view template \"{}\" controls this view's graphics; editing the template.",
            t.name
        );
        open_for_template(app, t.id);
        return;
    }
    app.dialogs.vg = Some(VgDialog {
        target: VgTarget::View(view.id),
        rows: rows_of(&view.graphics),
        annotation: false,
    });
}

pub fn open_for_template(app: &mut HausitApp, id: ElementId) {
    let Some(t) = app.doc.view_template(id) else { return };
    app.dialogs.vg = Some(VgDialog {
        target: VgTarget::Template(id),
        rows: rows_of(t.graphics.as_ref().unwrap_or(&Graphics::new())),
        annotation: false,
    });
}

fn weight_edit(ui: &mut Ui, id: &str, w: &mut Option<u8>, default: u8) {
    let text = match w {
        Some(v) => v.to_string(),
        None => "By Object Style".into(),
    };
    egui::ComboBox::from_id_salt(id).selected_text(text).width(100.0).show_ui(ui, |ui| {
        ui.selectable_value(w, None, format!("By Object Style ({default})"));
        for k in 1..=16u8 {
            ui.selectable_value(w, Some(k), k.to_string());
        }
    });
}

fn color_edit(ui: &mut Ui, c: &mut Option<[u8; 3]>, default: [u8; 3]) {
    ui.horizontal(|ui| {
        let mut on = c.is_some();
        if ui.checkbox(&mut on, "").on_hover_text("Override the colour").changed() {
            *c = on.then_some(c.unwrap_or(default));
        }
        match c {
            Some(rgb) => {
                ui.color_edit_button_srgb(rgb);
            }
            None => {
                let [r, g, b] = default;
                let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 16.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(r, g, b).gamma_multiply(0.4));
            }
        }
    });
}

pub fn show_vg(app: &mut HausitApp, ctx: &Context) {
    let Some(d) = app.dialogs.vg.as_mut() else { return };
    let title = match d.target {
        VgTarget::View(v) => format!(
            "Visibility/Graphics Overrides for {}",
            app.doc.view(v).map(|v| v.name.clone()).unwrap_or_default()
        ),
        VgTarget::Template(t) => format!(
            "Visibility/Graphics Overrides for View Template: {}",
            app.doc.view_template(t).map(|t| t.name.clone()).unwrap_or_default()
        ),
    };
    let mut result = None;
    egui::Window::new(title)
        .id(egui::Id::unique("vg_dialog"))
        .collapsible(false)
        .resizable(true)
        .default_size([760.0, 560.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut d.annotation, false, "Model Categories");
                ui.selectable_value(&mut d.annotation, true, "Annotation Categories");
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                egui::Grid::new("vg_grid")
                    .num_columns(7)
                    .striped(true)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        for h in [
                            "Visibility",
                            "Projection: colour",
                            "Projection: weight",
                            "Cut: colour",
                            "Cut: weight",
                            "Halftone",
                            "",
                        ] {
                            ui.label(RichText::new(h).strong());
                        }
                        ui.end_row();
                        let annotation = d.annotation;
                        for (cat, g) in d.rows.iter_mut().filter(|(c, _)| is_annotation_tab(*c) == annotation) {
                            let style = app.doc.object_styles.get(*cat);
                            ui.checkbox(&mut g.visible, cat.name());
                            color_edit(ui, &mut g.projection_color, style.color);
                            ui.push_id(("vgp", *cat), |ui| weight_edit(ui, "w", &mut g.projection_weight, style.projection));
                            if cat.is_model() {
                                color_edit(ui, &mut g.cut_color, style.color);
                                ui.push_id(("vgc", *cat), |ui| weight_edit(ui, "w", &mut g.cut_weight, style.cut));
                            } else {
                                ui.label("");
                                ui.label("");
                            }
                            ui.checkbox(&mut g.halftone, "");
                            if !g.is_default() {
                                if ui.small_button("Reset").clicked() {
                                    *g = CategoryGraphics {
                                        visible: g.visible,
                                        ..Default::default()
                                    };
                                }
                            } else {
                                ui.label("");
                            }
                            ui.end_row();
                        }
                    });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("All visible").clicked() {
                    let annotation = d.annotation;
                    for (c, g) in d.rows.iter_mut() {
                        if is_annotation_tab(*c) == annotation {
                            g.visible = true;
                        }
                    }
                }
                ui.label(
                    RichText::new("Unticked categories are hidden. Overrides replace Object Styles in this view only.")
                        .color(Color32::DARK_GRAY),
                );
            });
            ui.horizontal(|ui| {
                if ui.button("   OK   ").clicked() {
                    result = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    result = Some(false);
                }
            });
        });
    match result {
        Some(true) => {
            let d = app.dialogs.vg.take().unwrap();
            let graphics: Graphics = d.rows.into_iter().filter(|(_, g)| !g.is_default()).collect();
            app.transact("Visibility/Graphics", |doc| {
                match d.target {
                    VgTarget::View(v) => {
                        if let Some(v) = doc.view_mut(v) {
                            v.graphics = graphics;
                        }
                    }
                    VgTarget::Template(t) => {
                        if let Some(t) = doc.view_template_mut(t) {
                            t.graphics = Some(graphics);
                        }
                    }
                }
                Ok(())
            });
        }
        Some(false) => app.dialogs.vg = None,
        None => {}
    }
}

// ----- view templates ------------------------------------------------------------

pub struct TemplatesDialog {
    pub selected: Option<ElementId>,
    /// Create Template from Current View: the name being typed.
    pub new_name: String,
}

pub fn open_templates(app: &mut HausitApp) {
    app.dialogs.templates = Some(TemplatesDialog {
        selected: app.doc.view_templates.first().map(|t| t.id),
        new_name: String::new(),
    });
}

/// Apply Template Properties: copies a template's settings into the active
/// view once, without linking them.
pub fn apply_once(app: &mut HausitApp, template: ElementId) {
    let Some(view) = app.active_view else { return };
    let Some(t) = app.doc.view_template(template).cloned() else {
        return;
    };
    app.transact("Apply Template Properties", |doc| {
        if let Some(v) = doc.view_mut(view) {
            t.apply_to(v);
        }
        Ok(())
    });
    app.status = format!("Applied the properties of \"{}\" to the view.", t.name);
}

pub fn show_templates(app: &mut HausitApp, ctx: &Context) {
    let Some(mut d) = app.dialogs.templates.take() else { return };
    let mut open = true;
    let mut edit_vg = None;
    egui::Window::new("View Templates")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(620.0)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(200.0);
                    ui.label(RichText::new("Names").strong());
                    for t in &app.doc.view_templates {
                        let used = app.doc.views().filter(|v| v.template == Some(t.id)).count();
                        let label = if used > 0 { format!("{} ({used})", t.name) } else { t.name.clone() };
                        if ui.selectable_label(d.selected == Some(t.id), label).clicked() {
                            d.selected = Some(t.id);
                        }
                    }
                    if app.doc.view_templates.is_empty() {
                        ui.label("No templates yet.");
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let sel = d.selected;
                        if ui.add_enabled(sel.is_some(), egui::Button::new("Duplicate")).clicked() {
                            if let Some(t) = sel.and_then(|s| app.doc.view_template(s)).cloned() {
                                let name = format!("{} Copy", t.name);
                                let new = app.transact("Duplicate View Template", |doc| Ok(doc.insert_view_template(ViewTemplate { name, ..t })));
                                d.selected = new.or(d.selected);
                            }
                        }
                        if ui.add_enabled(sel.is_some(), egui::Button::new("Delete")).clicked() {
                            if let Some(s) = sel {
                                app.transact("Delete View Template", |doc| {
                                    doc.delete_view_template(s);
                                    Ok(())
                                });
                                d.selected = app.doc.view_templates.first().map(|t| t.id);
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label("From the current view:");
                    ui.add(egui::TextEdit::singleline(&mut d.new_name).hint_text("New template name").desired_width(190.0));
                    let can = app.active_view.and_then(|v| app.doc.view(v)).is_some_and(|v| v.kind.is_graphical() && !matches!(v.kind, ViewKind::Sheet(_)));
                    if ui.add_enabled(can && !d.new_name.trim().is_empty(), egui::Button::new("Create Template")).clicked() {
                        let (name, view) = (d.new_name.trim().to_string(), app.active_view.unwrap_or_default());
                        let new = app.transact("Create View Template", |doc| Ok(doc.add_view_template(&name, view)));
                        if let Some(Some(id)) = new {
                            d.selected = Some(id);
                            d.new_name.clear();
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    let Some(mut t) = d.selected.and_then(|s| app.doc.view_template(s)).cloned() else {
                        ui.label("Select a template.");
                        return;
                    };
                    let before = t.clone();
                    ui.label(RichText::new("View Properties").strong());
                    egui::Grid::new("tpl_props").num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
                        ui.label(RichText::new("Parameter").strong());
                        ui.label(RichText::new("Value").strong());
                        ui.label(RichText::new("Include").strong());
                        ui.end_row();
                        ui.label("Name");
                        ui.text_edit_singleline(&mut t.name);
                        ui.label("");
                        ui.end_row();

                        ui.label("View Scale");
                        let mut on = t.scale.is_some();
                        let mut s = t.scale.unwrap_or(100);
                        ui.add_enabled(on, egui::DragValue::new(&mut s).range(1..=5000).prefix("1 : "));
                        ui.checkbox(&mut on, "");
                        t.scale = on.then_some(s);
                        ui.end_row();

                        ui.label("Detail Level");
                        let mut on = t.detail_level.is_some();
                        let mut dl = t.detail_level.unwrap_or_default();
                        ui.add_enabled_ui(on, |ui| {
                            egui::ComboBox::from_id_salt("tpl_dl").selected_text(dl.label()).show_ui(ui, |ui| {
                                for x in DetailLevel::ALL {
                                    ui.selectable_value(&mut dl, x, x.label());
                                }
                            });
                        });
                        ui.checkbox(&mut on, "");
                        t.detail_level = on.then_some(dl);
                        ui.end_row();

                        ui.label("Visual Style");
                        let mut on = t.visual_style.is_some();
                        let mut vs = t.visual_style.unwrap_or_default();
                        ui.add_enabled_ui(on, |ui| {
                            egui::ComboBox::from_id_salt("tpl_vs").selected_text(vs.label()).show_ui(ui, |ui| {
                                for x in VisualStyle::ALL {
                                    ui.selectable_value(&mut vs, x, x.label());
                                }
                            });
                        });
                        ui.checkbox(&mut on, "");
                        t.visual_style = on.then_some(vs);
                        ui.end_row();

                        ui.label("Cut Plane (plans)");
                        let mut on = t.cut_height.is_some();
                        let mut c = t.cut_height.unwrap_or(1200.0);
                        ui.add_enabled(on, egui::DragValue::new(&mut c).range(0.0..=20000.0).suffix(" mm"));
                        ui.checkbox(&mut on, "");
                        t.cut_height = on.then_some(c);
                        ui.end_row();

                        ui.label("V/G Overrides");
                        let mut on = t.graphics.is_some();
                        let n = t.graphics.as_ref().map(|g| g.len()).unwrap_or(0);
                        if ui.add_enabled(on, egui::Button::new(format!("Edit… ({n} overridden)"))).clicked() {
                            edit_vg = Some(t.id);
                        }
                        ui.checkbox(&mut on, "");
                        t.graphics = if on { Some(t.graphics.clone().unwrap_or_default()) } else { None };
                        ui.end_row();
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Included settings are set by the template in every view that uses it, and cannot be changed in those views. Assign a template in a view's properties (View Template).")
                            .color(Color32::DARK_GRAY),
                    );
                    if app.active_view.is_some() && ui.button("Apply to the current view once").on_hover_text("Apply Template Properties: copy the settings without linking the view to the template").clicked() {
                        apply_once(app, t.id);
                    }
                    if t != before && !t.name.trim().is_empty() {
                        let id = t.id;
                        app.transact("Edit View Template", |doc| {
                            if let Some(slot) = doc.view_template_mut(id) {
                                *slot = t;
                            }
                            Ok(())
                        });
                    }
                });
            });
        });
    if let Some(t) = edit_vg {
        open_for_template(app, t);
    }
    if open {
        app.dialogs.templates = Some(d);
    }
}
