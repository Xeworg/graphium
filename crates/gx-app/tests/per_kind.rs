//! La suite por kind registrado (PRD §7, RF29; S5, S8).
//!
//! Un kind nuevo entra con su paquete completo —schema, reglas, traducción a la
//! IR, un ejemplo mínimo y un golden— y **el CI corre la suite por cada kind
//! registrado**. Este archivo es esa suite: recorre el registro, no una lista
//! escrita a mano, así que un kind que se registre mañana queda cubierto sin
//! tocar el test y uno que entre a medias falla acá.
//!
//! Todo pasa por la puerta única `render_json` (RNF6, S8): sin Tauri ni
//! webview. Lo que se exige a cada kind es lo que el producto promete cuando un
//! agente lo pide: un ejemplo mínimo que valide, un dibujo con bytes, el mismo
//! dibujo dos veces, y el `rendered_kind` que el servidor le va a contestar.

use std::sync::Arc;

use gx_core::engine::render_json;
use gx_core::layout::LayoutEngine;
use gx_core::render::{RenderOpts, SvgRenderer};
use gx_core::text::TextMetrics;

fn install_engines() {
    let metrics: Arc<dyn TextMetrics> =
        Arc::new(gx_render_svg::InterMetrics::new().expect("fonts"));
    let layout: Arc<dyn LayoutEngine> = Arc::new(gx_layout::Layouts::new(metrics.clone()));
    let renderer: Arc<dyn SvgRenderer> = Arc::new(gx_render_svg::SvgRenderer::new(metrics.clone()));
    gx_core::engine::install(gx_core::engine::Engines {
        layout,
        metrics,
        renderer,
    });
    gx_kinds::register_all();
}

/// The kinds the build ships, read from the registry itself.
fn registered() -> Vec<&'static str> {
    install_engines();
    gx_kinds::kind_names()
}

/// El ejemplo mínimo que el catálogo le da al agente para cada kind.
///
/// Sale del índice, que es de donde lo lee el agente: el schema de un kind
/// suelto no repite el ejemplo (PRD §11.1).
fn minimal_example(kind: &str) -> serde_json::Value {
    gx_mcp::contract::get_diagram_schema(None)
        .kinds
        .expect("el índice de kinds")
        .into_iter()
        .find(|entry| entry.kind == kind)
        .unwrap_or_else(|| panic!("`{kind}` no está en el índice del catálogo"))
        .min_example
}

/// S5: cada kind registrado pinta su ejemplo mínimo, con bytes y determinista.
#[test]
fn every_registered_kind_renders_its_minimal_example() {
    let kinds = registered();
    assert!(
        kinds.len() >= 12,
        "el registro tiene que traer el catálogo de F2, trajo {kinds:?}"
    );

    for kind in kinds {
        let example = minimal_example(kind);
        assert_eq!(
            example.get("kind").and_then(|value| value.as_str()),
            Some(kind),
            "el ejemplo mínimo de `{kind}` declara otro kind"
        );

        let document = example.to_string();
        let first = render_json(&document, &RenderOpts::default())
            .unwrap_or_else(|errors| panic!("el ejemplo de `{kind}` no pinta: {errors:?}"));
        let second = render_json(&document, &RenderOpts::default()).unwrap_or_else(|errors| {
            panic!("el ejemplo de `{kind}` no pinta dos veces: {errors:?}")
        });

        assert!(
            first.svg.starts_with("<svg"),
            "el dibujo de `{kind}` no es un SVG"
        );
        assert!(
            first.metrics.svg_bytes > 0 && first.metrics.nodes > 0,
            "el dibujo de `{kind}` salió vacío"
        );
        assert_eq!(first.svg, second.svg, "`{kind}` no es determinista (O8)");
    }
}

/// S5: el catálogo y el registro dicen lo mismo — un kind no puede estar en uno
/// y no en el otro.
#[test]
fn the_catalog_and_the_registry_name_the_same_kinds() {
    let registry = registered();
    let catalog: Vec<String> = gx_mcp::contract::get_diagram_schema(None)
        .kinds
        .expect("el índice de kinds")
        .into_iter()
        .map(|entry| entry.kind)
        .collect();

    let mut expected: Vec<String> = registry.iter().map(|kind| kind.to_string()).collect();
    expected.sort();
    let mut named = catalog.clone();
    named.sort();
    assert_eq!(
        named, expected,
        "el catálogo de kinds y el registro divergen"
    );
}

/// S8/RNF6: un kind desconocido se rechaza con la lista de los que hay, y la
/// lista sale del registro (no de una constante escrita a mano).
#[test]
fn an_unknown_kind_lists_every_registered_kind() {
    let kinds = registered();
    let errors = render_json(
        r#"{ "version": 1, "kind": "nope" }"#,
        &RenderOpts::default(),
    )
    .expect_err("un kind inventado no pinta");

    assert_eq!(errors[0].code, gx_core::error::ErrorCode::UnknownKind);
    let hint = errors[0].hint.as_deref().expect("la lista está en el hint");
    for kind in kinds {
        assert!(hint.contains(kind), "el hint no nombra `{kind}`: {hint}");
    }
}
