use std::collections::HashSet;

use crate::domain::identity::ElementId;
use crate::domain::property::PropertyValue;
use crate::repository::query::{ContextResult, ElementView};

/// Shortens a type ID string (e.g. "kat.core/requirement" -> "requirement").
fn short_type(type_id: &str) -> &str {
    type_id.strip_prefix("kat.core/").unwrap_or(type_id)
}

/// Shortens an ElementId or UUID to its first 8 hex characters.
fn short_id(id: &ElementId) -> String {
    let s = id.to_string();
    s.chars().take(8).collect()
}

/// Extracts title string from KnowledgeElementVersion properties.
fn element_title(ev: &crate::domain::element::KnowledgeElementVersion) -> &str {
    for (k, v) in &ev.properties {
        if k.as_str() == "title"
            && let PropertyValue::Text(t) = v
        {
            return t.as_str();
        }
    }
    "<untitled>"
}

/// Categorization buckets for Context presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextCategory {
    Requirements,
    Constraints,
    Design,
    Implementations,
    Artifacts,
    Validations,
    Other,
}

impl ContextCategory {
    fn label(&self) -> &'static str {
        match self {
            ContextCategory::Requirements => "Requirements",
            ContextCategory::Constraints => "Constraints",
            ContextCategory::Design => "Design Decisions",
            ContextCategory::Implementations => "Implementations",
            ContextCategory::Artifacts => "Artifacts",
            ContextCategory::Validations => "Validations",
            ContextCategory::Other => "Other Elements",
        }
    }

    fn code(&self) -> &'static str {
        match self {
            ContextCategory::Requirements => "REQ",
            ContextCategory::Constraints => "CON",
            ContextCategory::Design => "DES",
            ContextCategory::Implementations => "IMP",
            ContextCategory::Artifacts => "ART",
            ContextCategory::Validations => "VAL",
            ContextCategory::Other => "OTH",
        }
    }

    fn classify(type_id: &str) -> Self {
        let t = short_type(type_id);
        match t {
            "requirement" | "goal" | "use-case" | "user-story" => ContextCategory::Requirements,
            "constraint" | "restriction" | "policy" => ContextCategory::Constraints,
            "design-decision" | "architecture" | "decision" | "model" | "design" => {
                ContextCategory::Design
            }
            "implementation" | "code" | "module" | "service" => ContextCategory::Implementations,
            "artifact" | "file" => ContextCategory::Artifacts,
            "test" | "verification" | "validation" | "benchmark" => ContextCategory::Validations,
            _ => ContextCategory::Other,
        }
    }
}

/// Extracts physical locator (path/file/uri/location) explicitly modeled on an element or in physical routes.
fn find_physical_locator<'a>(ev: &'a ElementView, res: &'a ContextResult) -> Option<&'a str> {
    for (prop_key, prop_val) in &ev.element.properties {
        let k = prop_key.as_str().to_lowercase();
        if (k == "path" || k == "file" || k == "uri" || k == "location")
            && let PropertyValue::Text(val) = prop_val
        {
            return Some(val.as_str());
        }
    }

    for route in &res.physical_routes {
        if route.element_id == ev.element_id {
            return Some(route.path.as_str());
        }
    }

    None
}

/// Presents human-readable Context output.
pub fn present_human_context(res: &ContextResult, compact: bool, max_depth: Option<usize>) {
    let in_context_ids: HashSet<_> = res.elements.iter().map(|e| e.element_id).collect();

    if compact {
        // Compact single-line per element layout
        for ev in &res.elements {
            let cat = ContextCategory::classify(&ev.element.type_id);
            let code = cat.code();
            let sid = short_id(&ev.element_id);
            let title = element_title(&ev.element);
            let locator = find_physical_locator(ev, res);
            let display_name = locator.unwrap_or(title);

            let mut prov_parts = Vec::new();
            for rel in &ev.relationships.outgoing {
                if in_context_ids.contains(&rel.target_element_id) {
                    prov_parts.push(format!(
                        "{} -> {}",
                        short_type(&rel.relationship_type_id),
                        short_id(&rel.target_element_id)
                    ));
                }
            }
            for rel in &ev.relationships.incoming {
                if in_context_ids.contains(&rel.source_element_id) {
                    prov_parts.push(format!(
                        "{} <- {}",
                        short_type(&rel.relationship_type_id),
                        short_id(&rel.source_element_id)
                    ));
                }
            }

            if prov_parts.is_empty() {
                println!("{code}  {sid}  {display_name}");
            } else {
                let prov = prov_parts.join(", ");
                println!("{code}  {sid}  {display_name} ({prov})");
            }
        }

        if let Some(depth) = max_depth {
            println!("Context truncated at max depth {depth}.");
        }
        return;
    }

    // Default categorized development-oriented projection
    println!("Context");
    println!();

    // 1. Root section
    println!("Root");
    for root_id in &res.roots {
        if let Some(ev) = res.elements.iter().find(|e| &e.element_id == root_id) {
            let st = short_type(&ev.element.type_id);
            let sid = short_id(&ev.element_id);
            let title = element_title(&ev.element);
            println!("  [{st}] {sid}  {title}");
        }
    }
    println!();

    // 2. Category sections
    let categories = [
        ContextCategory::Requirements,
        ContextCategory::Constraints,
        ContextCategory::Design,
        ContextCategory::Implementations,
        ContextCategory::Artifacts,
        ContextCategory::Validations,
        ContextCategory::Other,
    ];

    for cat in categories {
        let matching_elements: Vec<_> = res
            .elements
            .iter()
            .filter(|ev| ContextCategory::classify(&ev.element.type_id) == cat)
            .collect();

        if matching_elements.is_empty() {
            continue;
        }

        println!("{}", cat.label());
        for ev in matching_elements {
            let st = short_type(&ev.element.type_id);
            let sid = short_id(&ev.element_id);
            let title = element_title(&ev.element);
            println!("  [{st}] {sid}  {title}");

            if let Some(locator) = find_physical_locator(ev, res) {
                println!("    path: {locator}");
            }

            // Render provenance: relationship hops connecting to other elements in the result set
            for rel in &ev.relationships.outgoing {
                if in_context_ids.contains(&rel.target_element_id) {
                    let rtype = short_type(&rel.relationship_type_id);
                    let target_sid = short_id(&rel.target_element_id);
                    println!("    {rtype} -> {target_sid}");
                }
            }

            for rel in &ev.relationships.incoming {
                if in_context_ids.contains(&rel.source_element_id) {
                    let rtype = short_type(&rel.relationship_type_id);
                    let source_sid = short_id(&rel.source_element_id);
                    println!("    {rtype} <- {source_sid}");
                }
            }
        }
        println!();
    }

    if let Some(depth) = max_depth {
        println!("Context truncated at max depth {depth}.");
    }
}
