# F2 — Cajas anidadas: dagre resuelve las hojas y el árbol se cierra en `gx-layout`

Corte que cierra el **T8** de `odd/tasks/f2-clusters.md`: los grupos anidan
desde el corte de clusters, pero el motor deja nodos sin posición cuando un
grupo contiene a otro y hay aristas que cruzan grupos. Acá se arregla sin
forkear la dependencia y sin devolver nunca un dibujo incompleto.

Criterio de salida: un documento con grupos en dos o tres niveles coloca
**todos** sus nodos, dibuja una caja por grupo con contención padre ⊇ hijo,
el caso de la matriz con grupo externo queda en verde, el único golden que
puede moverse es `component-nested`, y suite + clippy + fmt verdes.

## Specs

- **S1** (PRD §7 RF15, verbatim): «**RF15.** Grupos anidados (clusters) en el motor de grafo.»
- **S2** (PRD §7 RNF1-bis, verbatim): «**RNF1-bis — Matriz de layout (F0 la produce, F1 la gatea).** Se mide con 4 casos fijos por motor, no con un número suelto: `100n/150e` sin grupos · `100n/400e` sin grupos · `100n/150e` + 8 clusters · `300n/600e` sin grupos. Objetivo p95 en la máquina de referencia del CI (Linux x86_64, 4 vCPU, 8 GB, SSD): **< 500 ms** para los tres primeros y **< 1.5 s** para el de 300 nodos, con `DagreEngine` en frío (no hay caché de layout, por diseño determinista).»
- **S3** (defecto, verbatim de `odd/tasks/f2-clusters.md`, T8): «los clusters anidados con aristas que cruzan grupos pierden posiciones de nodo en `dagre` a escala. Reproducción: el caso RNF1-bis `100n/150e + 8 clusters` con un grupo externo que contiene a los 8 coloca **82 de 100** nodos (`n2`, `n6`, `n17`, …); sin el grupo externo coloca 100/100; sin aristas, 96 nodos en 8 clusters anidados se colocan 96/96.»
- **S4** (causa raíz, verificada leyendo `dagre` 0.1.2): `build_layer_matrix` (`layout/util.rs:155-181`) escribe `layers[rank][order] = v` **sin buscar hueco libre**; dos nodos de la misma capa con el mismo `order` colisionan y el que se escribe después borra al otro. `position_y`/`position_x` (`layout/position/mod.rs:58,71`) sólo escriben coordenadas para los nodos presentes en esa matriz, y `translate_graph` ignora los `None`. Los `order` duplicados aparecen porque los nodos compuestos entran en las listas de cada capa que abarcan (`layout/order/mod.rs:60-79`, activo recién cuando un grupo anida) y `do_resolve_conflicts` (`layout/order/resolve_conflicts.rs:91-95,122`) descarta los que quedan dentro de un ciclo de restricciones. Es un defecto de dagre.js, no del port: issue #236 abierto desde 2018-03-22 y PR #429 cerrado sin mergear.
- **S5** (decisión de este corte — a `dagre` sólo los clusters hoja): se registran como nodos compuestos únicamente los grupos cuyos miembros son **nodos**; los miembros de cualquier grupo siguen colgando de su cluster hoja. Es la topología que la matriz ya mide en 100/100.
- **S6** (decisión de este corte — cajas ancestro locales): la caja de un grupo que contiene otros grupos se calcula en `gx-layout` como la **unión de las cajas de sus miembros + padding**, de adentro hacia afuera, sin depender de `dagre`. Determinista: misma entrada, mismos bytes.
- **S7** (decisión de este corte — nunca un dibujo incompleto): si después de `layout()` la cantidad de nodos colocados es menor que la de la IR, se reintenta con `compound: false` y se calculan **todas** las cajas localmente. `LayoutGraph` no puede omitir un nodo de la IR en silencio.
- **S8** (decisión de este corte — evidencia obligatoria): un test de invariante `layout.nodes.len() == ir.nodes.len()` para el caso anidado; un quinto caso de matriz `100n/150e + 8 clusters + grupo externo`; y el único golden que puede cambiar es `component-nested` (`component-api` y el resto quedan byte a byte iguales).

### Lo que este corte no promete

Dos cajas **hermanas** pueden superponerse si `dagre` intercala a sus clusters
hoja; la caja se pinta de fondo translúcido detrás de nodos y aristas, así que
eso es costo de legibilidad y no dibujo roto. Si algún día molesta, el que pelea
eso es el motor, no `gx-layout`.

## Tasks

| ID | Qué | Specs | Ruta | Commit |
|---|---|---|---|---|
| T1 | Diagnóstico empírico: instrumentar `(rank, order, x.is_some(), y.is_some())` en el caso `100n/150e + 8 clusters + grupo externo` y confirmar las colisiones de `order`. Sin artefacto enviado: la instrumentación se borra al terminar | S3, S4 | worker | pending |
| T2 | Registro sólo-hoja en `DagreEngine` + cajas ancestro locales (unión + padding, de adentro hacia afuera) | S5, S6 | worker | pending |
| T3 | Red de seguridad (`compound: false` + cajas locales), invariante en test y quinto caso de matriz | S7, S8 | worker | pending |
| T4 | Goldens (sólo `component-nested`), nota en el PRD §8.4 y reporte upstream **redactado, no publicado** | S1, S2, S8 | inline | pending |
| T5 | Cierre: rama `feature/f2-cajas-anidadas`, PR con cuerpo, merge sin tag | — | parent | pending |

## Log

- **L1** (el humano, verbatim): «explimae el punto uno a y b», después «usa 3
  agentes dales el problema y que lo analicen y te den posibles soluciones en
  base a que es mas escalable y mantenible» con el agregado «explore / agentes
  explore», y ante la propuesta de la opción A′: «si dale».
- **L2** (síntesis de los tres exploradores, 2026-11-04): el de arquitectura
  pedía vendorizar y parchear con híbrido como plan B y descartaba el layout
  plano porque sin compound no hay separación entre clusters; el de ecosistema
  pedía hacer el adaptador total (caja desde los miembros, ya lo hace Mermaid)
  y descartaba `elk-layout` por licencia EPL contra un workspace MIT; el de
  internals encontró la causa exacta y propuso darle a `dagre` sólo los clusters
  hoja. La opción A′ combina las tres: topología hoja (internals), cajas
  propias como camino normal (ecosistema) y la disciplina de invariantes y
  casos de matriz (arquitectura).
- **L3** (decisión del humano, 2026-11-04): «si dale» a A′, con T1 de
  diagnóstico empírico primero y sin tocar la dependencia.
