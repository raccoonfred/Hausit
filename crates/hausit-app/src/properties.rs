//! The Properties palette: type selector, "Edit Type", and instance
//! parameters grouped like Revit. With nothing selected it shows the
//! active view's properties.

use crate::app::HausitApp;
use crate::options_bar::length_edit;
use egui::{Color32, RichText, Ui};
use hausit_core::ElementId;
use hausit_model::params::{element_parameters, set_element_parameter, ParamGroup, ParamKind, ParamValue};
use hausit_model::{Category, ViewKind};

fn header(ui: &mut Ui, text: &str) {
    egui::Frame::new()
        .fill(crate::theme::HEADER_BG)
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(text).strong());
        });
}

pub fn show(app: &mut HausitApp, ui: &mut Ui) {
    header(ui, "Properties");
    egui::ScrollArea::vertical()
        .id_salt("props_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(4.0);
            let elements: Vec<ElementId> = app.selection.iter().copied().filter(|id| app.doc.element(*id).is_some()).collect();
            let cats: Vec<Category> = elements
                .iter()
                .filter_map(|id| app.doc.element(*id))
                .map(|e| e.category())
                .collect();
            let single_cat = (!cats.is_empty() && cats.iter().all(|c| *c == cats[0])).then(|| cats[0]);

            // Type selector: for the selection, or for the active placement tool.
            let selector_cat = single_cat.or_else(|| if elements.is_empty() { app.tool.category() } else { None });
            if let Some(cat) = selector_cat.filter(|c| Category::TYPED.contains(c)) {
                type_selector(app, ui, cat, &elements);
            } else if elements.len() > 1 {
                ui.label(RichText::new(format!("Common ({})", elements.len())).strong());
            }

            if elements.is_empty() {
                if let Some(v) = app
                    .selection
                    .first()
                    .copied()
                    .filter(|id| app.doc.view(*id).is_some())
                    .or(app.active_view)
                {
                    view_properties(app, ui, v);
                }
                return;
            }
            if single_cat.is_none() {
                return;
            }
            instance_parameters(app, ui, &elements);
        });
}

fn type_selector(app: &mut HausitApp, ui: &mut Ui, cat: Category, elements: &[ElementId]) {
    let current = if elements.is_empty() {
        app.type_for(cat)
    } else {
        let t = app.doc.element(elements[0]).and_then(|e| e.type_id());
        if elements.iter().all(|id| app.doc.element(*id).and_then(|e| e.type_id()) == t) {
            t
        } else {
            None
        }
    };
    // Isolated and wall foundations cannot swap types with each other.
    let wall_foundation = match elements.first().and_then(|id| app.doc.element(*id)).map(|e| &e.data) {
        Some(hausit_model::ElementData::WallFooting(_)) => Some(true),
        Some(hausit_model::ElementData::Footing(_)) => Some(false),
        _ => app.tool.wall_foundation(),
    };
    let types: Vec<(ElementId, String, String)> = app
        .doc
        .types_of(cat)
        .filter(|t| cat != Category::StructuralFoundations || wall_foundation.is_none_or(|w| t.is_wall_foundation() == w))
        .map(|t| (t.id, t.family.clone(), t.name.clone()))
        .collect();
    let current_label = current
        .and_then(|c| app.doc.family_type(c))
        .map(|t| format!("{}\n{}", t.family, t.name))
        .unwrap_or_else(|| "<varies>".into());
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(200)))
        .inner_margin(4)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let mut picked = None;
            egui::ComboBox::from_id_salt("type_selector")
                .selected_text(RichText::new(current_label).strong())
                .width(ui.available_width() - 8.0)
                .height(400.0)
                .show_ui(ui, |ui| {
                    let mut last_family = String::new();
                    for (id, family, name) in &types {
                        if *family != last_family {
                            ui.label(RichText::new(family).strong().color(Color32::DARK_GRAY));
                            last_family = family.clone();
                        }
                        if ui.selectable_label(current == Some(*id), format!("    {name}")).clicked() {
                            picked = Some(*id);
                        }
                    }
                });
            if let Some(t) = picked {
                if elements.is_empty() {
                    app.current_type.insert(cat, t);
                } else {
                    let ids = elements.to_vec();
                    app.transact("Change Type", |doc| {
                        for id in &ids {
                            doc.change_type(*id, t)?;
                        }
                        Ok(())
                    });
                }
            }
        });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{} ({})",
                if elements.is_empty() {
                    format!("New {}", cat.name())
                } else {
                    cat.name().to_string()
                },
                elements.len().max(1)
            ))
            .color(Color32::DARK_GRAY),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(current.is_some(), egui::Button::new("Edit Type")).clicked() {
                if let Some(t) = current {
                    app.dialogs.open_type_properties(&app.doc, t);
                }
            }
        });
    });
}

fn instance_parameters(app: &mut HausitApp, ui: &mut Ui, elements: &[ElementId]) {
    let params = element_parameters(&app.doc, elements[0]);
    let unit = app.doc.length_unit;
    let levels = app.doc.levels().into_iter().map(|(id, l)| (id, l.name.clone())).collect::<Vec<_>>();
    let mut edit: Option<(&'static str, ParamValue)> = None;
    let mut groups: Vec<(ParamGroup, Vec<&hausit_model::Parameter>)> = Vec::new();
    for p in &params {
        match groups.last_mut() {
            Some((g, v)) if *g == p.group => v.push(p),
            _ => groups.push((p.group, vec![p])),
        }
    }
    for (group, ps) in groups {
        egui::CollapsingHeader::new(RichText::new(group.label()).strong())
            .id_salt(group.label())
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new(("params", group.label()))
                    .num_columns(2)
                    .striped(true)
                    .min_col_width(ui.available_width() * 0.42)
                    .show(ui, |ui| {
                        for p in ps {
                            ui.label(p.name);
                            ui.add_enabled_ui(!p.read_only, |ui| {
                                let id_key = format!("{}-{}", elements[0], p.name);
                                match (&p.kind, &p.value) {
                                    (ParamKind::Level | ParamKind::LevelOrNone, ParamValue::Element(cur)) => {
                                        let mut v = *cur;
                                        let name = if v.is_valid() {
                                            app.doc.level_name(v)
                                        } else {
                                            "Unconnected".into()
                                        };
                                        egui::ComboBox::from_id_salt(&id_key)
                                            .selected_text(name)
                                            .width(130.0)
                                            .show_ui(ui, |ui| {
                                                if p.kind == ParamKind::LevelOrNone {
                                                    ui.selectable_value(&mut v, ElementId::INVALID, "Unconnected");
                                                }
                                                for (id, n) in &levels {
                                                    ui.selectable_value(
                                                        &mut v,
                                                        *id,
                                                        if p.kind == ParamKind::LevelOrNone {
                                                            format!("Up to level: {n}")
                                                        } else {
                                                            n.clone()
                                                        },
                                                    );
                                                }
                                            });
                                        if v != *cur {
                                            edit = Some((p.name, ParamValue::Element(v)));
                                        }
                                    }
                                    (ParamKind::Choice(options), ParamValue::Choice(i)) => {
                                        let mut v = *i;
                                        egui::ComboBox::from_id_salt(&id_key)
                                            .selected_text(options.get(v).cloned().unwrap_or_default())
                                            .width(130.0)
                                            .show_ui(ui, |ui| {
                                                for (k, o) in options.iter().enumerate() {
                                                    ui.selectable_value(&mut v, k, o);
                                                }
                                            });
                                        if v != *i {
                                            edit = Some((p.name, ParamValue::Choice(v)));
                                        }
                                    }
                                    (_, ParamValue::Length(v)) => {
                                        let mut x = *v;
                                        if length_edit(ui, &id_key, &mut x, unit) {
                                            edit = Some((p.name, ParamValue::Length(x)));
                                        }
                                    }
                                    (_, ParamValue::Angle(v)) => {
                                        let mut x = *v;
                                        if ui
                                            .add(egui::DragValue::new(&mut x).range(0.0..=75.0).suffix("°").speed(0.5))
                                            .changed()
                                        {
                                            edit = Some((p.name, ParamValue::Angle(x)));
                                        }
                                    }
                                    (_, ParamValue::Bool(b)) => {
                                        let mut x = *b;
                                        if ui.checkbox(&mut x, "").changed() {
                                            edit = Some((p.name, ParamValue::Bool(x)));
                                        }
                                    }
                                    (_, ParamValue::Text(s)) => {
                                        if p.read_only {
                                            ui.label(s);
                                        } else {
                                            let key = egui::Id::unique(("text", &id_key));
                                            let mut t = ui.ctx().data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| s.clone());
                                            let r = ui.add(egui::TextEdit::singleline(&mut t).desired_width(130.0));
                                            if r.has_focus() {
                                                ui.ctx().data_mut(|d| d.insert_temp(key, t.clone()));
                                            }
                                            if r.lost_focus() {
                                                ui.ctx().data_mut(|d| d.remove::<String>(key));
                                                if t != *s {
                                                    edit = Some((p.name, ParamValue::Text(t)));
                                                }
                                            }
                                        }
                                    }
                                    (_, ParamValue::Area(a)) => {
                                        ui.label(format!("{:.3} m²", a / 1e6));
                                    }
                                    (_, ParamValue::Volume(v)) => {
                                        ui.label(format!("{:.3} m³", v / 1e9));
                                    }
                                    (_, other) => {
                                        ui.label(format!("{other:?}"));
                                    }
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
    }
    if let Some((name, value)) = edit {
        let ids = elements.to_vec();
        app.transact(&format!("Change {name}"), |doc| {
            for id in &ids {
                set_element_parameter(doc, *id, name, value.clone())?;
            }
            Ok(())
        });
    }
}

fn view_properties(app: &mut HausitApp, ui: &mut Ui, view_id: ElementId) {
    let Some(view) = app.doc.view(view_id).cloned() else { return };
    let kind = match &view.kind {
        ViewKind::FloorPlan { .. } => "Floor Plan",
        ViewKind::CeilingPlan { .. } => "Reflected Ceiling Plan",
        ViewKind::ThreeD => "3D View",
        ViewKind::Elevation { .. } => "Elevation",
        ViewKind::Section { .. } => "Section",
        ViewKind::Schedule(_) => "Schedule",
    };
    ui.label(RichText::new(format!("{kind}: {}", view.name)).strong());
    ui.add_space(4.0);
    let mut v = view.clone();
    let unit = app.doc.length_unit;
    let header = if v.kind.is_graphical() { "Graphics" } else { "Extents" };
    egui::CollapsingHeader::new(RichText::new(header).strong())
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("view_props").num_columns(2).show(ui, |ui| {
                if v.kind.is_graphical() {
                    ui.label("View Scale");
                    ui.add(egui::DragValue::new(&mut v.scale).range(1..=5000).prefix("1 : "));
                    ui.end_row();
                    ui.label("Detail Level");
                    egui::ComboBox::from_id_salt("vp_detail")
                        .selected_text(v.detail_level.label())
                        .show_ui(ui, |ui| {
                            for d in hausit_model::DetailLevel::ALL {
                                ui.selectable_value(&mut v.detail_level, d, d.label());
                            }
                        });
                    ui.end_row();
                }
                if v.kind.plan_level().is_some() {
                    ui.label("Cut Plane (View Range)");
                    length_edit(ui, "vp_cut", &mut v.cut_height, unit);
                    ui.end_row();
                }
                if let ViewKind::Section { far_clip, .. } = &mut v.kind {
                    ui.label("Far Clip Offset");
                    length_edit(ui, "vp_far", far_clip, unit);
                    ui.end_row();
                }
            });
        });
    if matches!(v.kind, ViewKind::ThreeD) {
        egui::CollapsingHeader::new(RichText::new("Extents").strong())
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("view_extents").num_columns(2).show(ui, |ui| {
                    ui.label("Section Box");
                    let mut on = v.section_box.is_some();
                    if ui.checkbox(&mut on, "").changed() {
                        v.section_box = if on { crate::app::default_section_box(&app.doc) } else { None };
                    }
                    ui.end_row();
                    if let Some(b) = &mut v.section_box {
                        for (label, value) in [
                            ("Box Left", &mut b.min.x),
                            ("Box Right", &mut b.max.x),
                            ("Box Front", &mut b.min.y),
                            ("Box Back", &mut b.max.y),
                            ("Box Bottom", &mut b.min.z),
                            ("Box Top", &mut b.max.z),
                        ] {
                            ui.label(label);
                            length_edit(ui, &format!("vp_{label}"), value, unit);
                            ui.end_row();
                        }
                    }
                });
            });
    }
    egui::CollapsingHeader::new(RichText::new("Identity Data").strong())
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("view_identity").num_columns(2).show(ui, |ui| {
                ui.label("View Name");
                let key = egui::Id::unique(("view_name", view_id));
                let mut t = ui.ctx().data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| v.name.clone());
                let r = ui.add(egui::TextEdit::singleline(&mut t).desired_width(140.0));
                if r.has_focus() {
                    ui.ctx().data_mut(|d| d.insert_temp(key, t.clone()));
                }
                if r.lost_focus() {
                    ui.ctx().data_mut(|d| d.remove::<String>(key));
                    if !t.trim().is_empty() {
                        v.name = t;
                    }
                }
                ui.end_row();
            });
        });
    if let ViewKind::Schedule(_) = v.kind {
        crate::schedule_view::properties(app, ui, view_id);
        return;
    }
    if v != view {
        app.transact("Change View Properties", |doc| {
            if let Some(t) = doc.view_mut(view_id) {
                *t = v;
            }
            Ok(())
        });
    }
}
