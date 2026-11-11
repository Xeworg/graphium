//! `gx-kinds` — the diagram kinds of Graphium.
//!
//! A kind is a module with a JSON contract, its semantic rules and its
//! translation to the IR. Adding a kind never touches `gx-mcp` nor `gx-app`
//! (PRD §7, O7).

pub mod c4;
pub mod class;
pub mod component;
pub mod dataflow;
pub mod er;
pub mod flow;
pub mod flowchart;
pub mod gantt;
pub mod graph;
pub mod groups;
pub mod lanes;
pub mod mindmap;
pub mod sequence;
pub mod state;
pub mod swimlane;
pub mod timeline;
pub mod tree;

#[cfg(test)]
mod test_support;

/// Registers every kind of this crate in the global registry.
pub fn register_all() {
    graph::register();
    er::register();
    tree::register();
    mindmap::register();
    sequence::register();
    flowchart::register();
    class::register();
    component::register();
    state::register();
    dataflow::register();
    timeline::register();
    swimlane::register();
    // Los kinds nuevos entran al final: el registro los ordena por nombre.
    c4::register();
    gantt::register();
}

/// The names of the kinds this crate registers, read from the registry itself
/// (`f20-reuso.md`, S10): there is no second list to forget, so a kind that
/// `register_all` registers cannot escape the per-kind suite.
pub fn kind_names() -> Vec<&'static str> {
    register_all();
    gx_core::kind::kind_names()
}

#[cfg(test)]
mod tests {
    use super::{kind_names, register_all};

    /// S10 (`f20-reuso.md`): el catálogo lee el registro, así que no hay dos
    /// listas que puedan discrepar: un kind registrado y no listado ya no
    /// existe, y un kind nuevo entra sin editar `gx-mcp`.
    #[test]
    fn kind_names_reads_the_registry() {
        register_all();
        assert_eq!(kind_names(), gx_core::kind::kind_names());
        for name in kind_names() {
            assert!(
                gx_core::kind::for_kind(name).is_some(),
                "`{name}` está en la lista y no en el registro"
            );
        }
    }
}
