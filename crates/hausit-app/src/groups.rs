//! Model groups in the app: Create Group (GP), Edit Group with Add,
//! Remove, Finish and Cancel, Ungroup, and placing more instances from
//! the Project Browser.

use crate::app::HausitApp;
use crate::icons::Icon;
use crate::ribbon::{big, panel};
use crate::tools::{self, Tool};
use egui::{CollapsingHeader, Ui};
use hausit_core::ElementId;
use hausit_model::Category;

/// What clicking `id` selects: its group, unless that group is being
/// edited.
pub fn pick_target(app: &HausitApp, id: ElementId) -> ElementId {
    match app.doc.group_of(id) {
        Some(g) if app.group_edit != Some(g) => g,
        _ => id,
    }
}

/// Create Group from the selection.
pub fn create_group(app: &mut HausitApp) {
    let ids = app.selection.clone();
    let name = app.doc.next_group_name();
    if let Some(g) = app.transact("Create Group", |doc| doc.create_group(&ids, &name)) {
        app.selection = vec![g];
        app.status = format!("Created {name}. Edit Group changes every instance; Ungroup keeps the elements.");
    }
}

/// The contextual "Group" panel (or "Edit Group" while editing one).
pub fn group_panel(app: &mut HausitApp, ui: &mut Ui) {
    if let Some(g) = app.group_edit {
        panel(ui, "Edit Group", |ui| {
            let sel = app.selection.clone();
            let outside = sel.iter().any(|s| app.doc.group_of(*s).is_none() && *s != g);
            if big(ui, Icon::Group, "Add", "Add the selected elements to the group", false, outside) {
                app.transact("Add to Group", |doc| doc.add_to_group(g, &sel));
            }
            let inside = sel.iter().any(|s| app.doc.group_of(*s) == Some(g));
            if big(
                ui,
                Icon::Group,
                "Remove",
                "Remove the selected elements from the group",
                false,
                inside,
            ) {
                app.transact("Remove from Group", |doc| {
                    doc.remove_from_group(g, &sel);
                    Ok(())
                });
            }
            if big(
                ui,
                Icon::Finish,
                "Finish",
                "Finish: every instance of this group takes the change",
                false,
                true,
            ) {
                finish_edit(app);
            }
            if big(
                ui,
                Icon::Cancel,
                "Cancel",
                "Cancel: undo the changes made to the group",
                false,
                true,
            ) {
                if let Some(before) = app.group_edit_before.take() {
                    app.transact("Cancel Edit Group", |doc| {
                        *doc = *before;
                        Ok(())
                    });
                }
                app.group_edit = None;
                app.selection = vec![g];
            }
        });
        return;
    }
    let group = match app.selection.as_slice() {
        [one] if app.doc.group(*one).is_some() => Some(*one),
        _ => None,
    };
    if group.is_none() && app.selection.is_empty() {
        return;
    }
    panel(ui, "Group", |ui| match group {
        Some(g) => {
            if big(
                ui,
                Icon::Group,
                "Edit Group",
                "Edit Group: change the members of every instance",
                false,
                true,
            ) {
                app.group_edit_before = Some(Box::new(app.doc.clone()));
                app.group_edit = Some(g);
                app.selection.clear();
                app.status = "Editing the group: select and change its members, then Finish.".into();
            }
            if big(
                ui,
                Icon::Group,
                "Ungroup",
                "Ungroup: keep the members as separate elements",
                false,
                true,
            ) {
                if let Some(members) = app.transact("Ungroup", |doc| Ok(doc.ungroup(g))) {
                    app.selection = members;
                }
            }
        }
        None => {
            if big(
                ui,
                Icon::Group,
                "Create Group",
                "Create Group (GP) from the selected elements",
                false,
                true,
            ) {
                create_group(app);
            }
        }
    });
}

/// Finish editing: copies the edited instance to every other one.
pub fn finish_edit(app: &mut HausitApp) {
    if let Some(g) = app.group_edit.take() {
        app.transact("Edit Group", |doc| {
            doc.sync_group(g);
            Ok(())
        });
        app.group_edit_before = None;
        app.selection = vec![g];
    }
}

/// "Groups" in the Project Browser: each group type, with Place Instance.
pub fn browser_node(app: &mut HausitApp, ui: &mut Ui) {
    let types: Vec<(ElementId, String)> = app.doc.types_of(Category::ModelGroups).map(|t| (t.id, t.name.clone())).collect();
    CollapsingHeader::new("Groups").default_open(false).show(ui, |ui| {
        CollapsingHeader::new("Model").id_salt("groups_model").show(ui, |ui| {
            for (id, name) in types {
                let count = app.doc.group_instances(id).len();
                let r = ui.selectable_label(false, format!("{name} ({count})"));
                if r.double_clicked() {
                    tools::start(app, Tool::PlaceGroup { type_id: id });
                }
                r.context_menu(|ui| {
                    if ui.add_enabled(count > 0, egui::Button::new("Place Instance")).clicked() {
                        tools::start(app, Tool::PlaceGroup { type_id: id });
                        ui.close();
                    }
                    if ui.button("Select All Instances").clicked() {
                        app.selection = app.doc.group_instances(id);
                        ui.close();
                    }
                    if ui.button("Rename…").clicked() {
                        app.dialogs.rename = Some(crate::dialogs::Rename {
                            target: id,
                            text: name.clone(),
                            is_type: true,
                        });
                        ui.close();
                    }
                });
            }
        });
    });
}
