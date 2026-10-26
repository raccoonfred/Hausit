//! Schedules: tabular, live views of model elements (Revit's
//! "Schedule/Quantities").

use hausit_core::{format_length, ElementId, LengthUnit};
use hausit_model::{params::element_parameters, Document, ElementData, ParamValue, ScheduleDef, ScheduleField, TopConstraint};

/// A computed cell: the display text and, for numeric fields, the value
/// used for sorting and totals.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub text: String,
    pub number: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub cells: Vec<Cell>,
    /// Elements represented by this row (several when not itemized).
    pub elements: Vec<ElementId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Row>,
    pub totals: Option<Vec<String>>,
}

fn area_m2(mm2: f64) -> String {
    format!("{:.2} m²", mm2 / 1e6)
}

fn volume_m3(mm3: f64) -> String {
    format!("{:.2} m³", mm3 / 1e9)
}

fn param(doc: &Document, id: ElementId, name: &str) -> Option<ParamValue> {
    element_parameters(doc, id).into_iter().find(|p| p.name == name).map(|p| p.value)
}

fn cell(doc: &Document, id: ElementId, field: ScheduleField, unit: LengthUnit) -> Cell {
    use ScheduleField as F;
    let text = |s: String| Cell { text: s, number: None };
    let len = |v: f64| Cell {
        text: format_length(v, unit),
        number: Some(v),
    };
    let Some(e) = doc.element(id) else { return text(String::new()) };
    let ty = doc.type_of(id);
    let from_param = |name: &str| match param(doc, id, name) {
        Some(ParamValue::Length(v)) => len(v),
        Some(ParamValue::Area(v)) => Cell {
            text: area_m2(v),
            number: Some(v),
        },
        Some(ParamValue::Volume(v)) => Cell {
            text: volume_m3(v),
            number: Some(v),
        },
        Some(ParamValue::Angle(v)) => Cell {
            text: format!("{v:.2}°"),
            number: Some(v),
        },
        Some(ParamValue::Element(l)) => text(doc.level_name(l)),
        Some(ParamValue::Text(s)) => text(s),
        Some(ParamValue::Bool(b)) => text(if b { "Yes".into() } else { "No".into() }),
        _ => text(String::new()),
    };
    match field {
        F::Family => text(ty.map(|t| t.family.clone()).unwrap_or_default()),
        F::Type => text(ty.map(|t| t.name.clone()).unwrap_or_default()),
        F::FamilyAndType => text(ty.map(|t| t.display_name()).unwrap_or_default()),
        F::TypeMark => text(match ty.and_then(|t| t.params.get("Type Mark")) {
            Some(ParamValue::Text(s)) => s.clone(),
            _ => String::new(),
        }),
        F::Mark => Cell {
            text: e.mark.clone(),
            number: e.mark.parse().ok(),
        },
        F::Comments => text(e.comments.clone()),
        F::Count => Cell {
            text: "1".into(),
            number: Some(1.0),
        },
        F::Level => from_param(match e.data {
            ElementData::Roof(_) | ElementData::Column(_) => "Base Level",
            _ => "Level",
        }),
        F::Name => from_param("Name"),
        F::Number => {
            let c = from_param("Number");
            Cell {
                number: c.text.parse().ok(),
                ..c
            }
        }
        F::UnboundedHeight => from_param("Unbounded Height"),
        F::Length => from_param("Length"),
        F::Area => from_param("Area"),
        F::Volume => from_param("Volume"),
        F::Perimeter => from_param("Perimeter"),
        F::Width => len(ty
            .map(|t| {
                if e.category() == hausit_model::Category::Walls {
                    t.thickness()
                } else {
                    t.width()
                }
            })
            .unwrap_or(0.0)),
        F::Height => len(ty.map(|t| t.height()).unwrap_or(0.0)),
        F::Thickness => from_param("Thickness"),
        F::SillHeight => from_param("Sill Height"),
        F::HeadHeight => from_param("Head Height"),
        F::BaseConstraint => from_param("Base Constraint"),
        F::TopConstraint => match &e.data {
            ElementData::Wall(w) => text(match w.top {
                TopConstraint::Unconnected { .. } => "Unconnected".into(),
                TopConstraint::UpToLevel { level, .. } => format!("Up to level: {}", doc.level_name(level)),
            }),
            _ => text(String::new()),
        },
        F::UnconnectedHeight => from_param("Unconnected Height"),
        F::Host => from_param("Host"),
        F::Slope => from_param("Slope"),
        F::Structural => from_param("Structural"),
    }
}

/// Computes a schedule's table from the current model.
pub fn build_table(doc: &Document, def: &ScheduleDef) -> Table {
    let unit = doc.length_unit;
    let mut rows: Vec<Row> = doc
        .elements_of(def.category)
        .map(|e| Row {
            cells: def.fields.iter().map(|f| cell(doc, e.id, *f, unit)).collect(),
            elements: vec![e.id],
        })
        .collect();

    if let Some(key) = def.sort_by.and_then(|k| def.fields.iter().position(|f| *f == k)) {
        rows.sort_by(|a, b| match (a.cells[key].number, b.cells[key].number) {
            (Some(x), Some(y)) => x.total_cmp(&y),
            _ => natural_cmp(&a.cells[key].text, &b.cells[key].text),
        });
    }

    if !def.itemize {
        // Group rows whose non-summable cells are identical.
        let mut grouped: Vec<Row> = Vec::new();
        for r in rows {
            let key = |row: &Row| -> Vec<String> {
                def.fields
                    .iter()
                    .zip(&row.cells)
                    .filter(|(f, _)| !f.is_summable())
                    .map(|(_, c)| c.text.clone())
                    .collect()
            };
            if let Some(g) = grouped.iter_mut().find(|g| key(g) == key(&r)) {
                for (i, f) in def.fields.iter().enumerate() {
                    if f.is_summable() {
                        let sum = g.cells[i].number.unwrap_or(0.0) + r.cells[i].number.unwrap_or(0.0);
                        g.cells[i] = Cell {
                            text: format_sum(*f, sum, unit),
                            number: Some(sum),
                        };
                    }
                }
                g.elements.extend(r.elements);
            } else {
                grouped.push(r);
            }
        }
        rows = grouped;
    }

    let totals = def.grand_totals.then(|| {
        def.fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                if f.is_summable() {
                    let sum: f64 = rows.iter().filter_map(|r| r.cells[i].number).sum();
                    format_sum(*f, sum, unit)
                } else if i == 0 {
                    "Grand total".into()
                } else {
                    String::new()
                }
            })
            .collect()
    });

    Table {
        headers: def.fields.iter().map(|f| f.label().to_string()).collect(),
        rows,
        totals,
    }
}

fn format_sum(f: ScheduleField, v: f64, unit: LengthUnit) -> String {
    match f {
        ScheduleField::Count => format!("{v}"),
        ScheduleField::Area => area_m2(v),
        ScheduleField::Volume => volume_m3(v),
        _ => format_length(v, unit),
    }
}

/// Compares strings so that "Door 2" sorts before "Door 10".
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.total_cmp(&y),
        _ => a.cmp(b),
    }
}

/// Exports a table as CSV (Revit: File ▸ Export ▸ Reports ▸ Schedule).
pub fn to_csv(table: &Table) -> String {
    let esc = |s: &str| {
        if s.contains([',', '"', '\n']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let mut out = String::new();
    let line = |cells: Vec<&str>| cells.into_iter().map(esc).collect::<Vec<_>>().join(",") + "\n";
    out += &line(table.headers.iter().map(String::as_str).collect());
    for r in &table.rows {
        out += &line(r.cells.iter().map(|c| c.text.as_str()).collect());
    }
    if let Some(t) = &table.totals {
        out += &line(t.iter().map(String::as_str).collect());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hausit_model::template::sample_project;
    use hausit_model::Category;

    #[test]
    fn door_schedule() {
        let doc = sample_project();
        let t = build_table(&doc, &ScheduleDef::new(Category::Doors));
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0].cells[0].text, "1");
        let count = t.headers.iter().position(|h| h == "Count").unwrap();
        assert_eq!(t.totals.as_ref().unwrap()[count], "2");
    }

    #[test]
    fn grouped_wall_schedule() {
        let doc = sample_project();
        let mut def = ScheduleDef::new(Category::Walls);
        def.itemize = false;
        let t = build_table(&doc, &def);
        assert_eq!(t.rows.len(), 2); // exterior and partition types
        let csv = to_csv(&t);
        assert!(csv.starts_with("Type,Base Constraint,Length,Area,Volume\n"));
    }
}
