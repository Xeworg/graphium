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
- **S4-bis** (matiz medido en T1, que corrige la lectura ingenua de S4): el
  sitio de la pérdida es `build_layer_matrix`, pero su rastro **no** se ve en
  las etiquetas devueltas. En el caso de reproducción, sólo 4 de los 18 nodos
  sin posición comparten `(rank, order)` con un nodo colocado en el resultado
  final; los otros 14 fueron pisados por nodos temporales (dummies de aristas y
  de borde) que se crean en los pasos 12/14 y se borran en 20/21 de
  `layout/mod.rs`. Consecuencia de diseño: **el detector es el invariante de S7**
  (nodos colocados contra nodos de la IR), no un escaneo de `(rank, order)`
  duplicados, que sólo atrapa al 22 % de los casos.
- **S5** (decisión de este corte — a `dagre` sólo los clusters hoja): se registran como nodos compuestos únicamente los grupos cuyos miembros son **nodos**; los miembros de cualquier grupo siguen colgando de su cluster hoja. Es la topología que la matriz ya mide en 100/100.
- **S6** (decisión de este corte — cajas ancestro locales): la caja de un grupo que contiene otros grupos se calcula en `gx-layout` como la **unión de las cajas de sus miembros + padding**, de adentro hacia afuera, sin depender de `dagre`. Determinista: misma entrada, mismos bytes.
- **S7** (decisión de este corte — nunca un dibujo incompleto): si después de `layout()` la cantidad de nodos colocados es menor que la de la IR, se reintenta con `compound: false` y se calculan **todas** las cajas localmente. `LayoutGraph` no puede omitir un nodo de la IR en silencio.
- **S8** (decisión de este corte — evidencia obligatoria): un test de invariante `layout.nodes.len() == ir.nodes.len()` para el caso anidado; un quinto caso de matriz `100n/150e + 8 clusters + grupo externo`; y el único golden que puede cambiar es `component-nested` (`component-api` y el resto quedan byte a byte iguales).

### Lo que este corte no promete

Dos cajas **hermanas** pueden superponerse si `dagre` intercala a sus clusters
hoja; la caja se pinta de fondo translúcido detrás de nodos y aristas, así que
eso es costo de legibilidad y no dibujo roto. Si algún día molesta, el que pelea
eso es el motor, no `gx-layout`.

## Reporte upstream (redactado para `dagre` 0.1.2, **no publicado**)

Título propuesto: *compound layout leaves nodes without coordinates when
children of a cluster are ranked through transitive dummies*.

Cuerpo, listo para pegar en un issue (no se abre sin permiso del humano porque
es un repositorio ajeno):

> `build_layer_matrix` (`src/layout/util.rs`, ~líneas 155-181) coloca cada nodo
> en `layers[rank][order]` sin buscar un hueco libre. Cuando dos nodos de la
> misma capa comparten `order` —y los nodos compuestos entran en las listas de
> **cada** capa que abarcan (`layout/order/mod.rs`, el bucle `min_rank..=max_rank`),
> y `do_resolve_conflicts` descarta los que caen en un ciclo de restricciones—
> el último pisa al primero, el pisado no entra en la matriz y `position_x`/
> `position_y` nunca le escriben coordenadas (`layout/position/mod.rs`);
> `translate_graph` ignora los `None` sin avisar.
>
> Reproducción: 100 nodos, 150 aristas, 8 clusters de 12 nodos más **un grupo
> externo que contiene a los 8**. Resultado: 82 de 100 nodos con `x` y `y` en
> `None`. Sin el grupo externo: 100/100. Anidando sin aristas: 96/96.
> `NodeLabel.padding` y el orden de las llamadas a `set_parent` no cambian nada.
>
> Parche mínimo (una función): en `build_layer_matrix`, si el slot `order` ya
> está ocupado, usar el primer slot libre de esa capa y reasignar `order`
> densos. Parche de causa (dos funciones): además, en `do_resolve_conflicts`,
> agregar al final del orden los nodos que nunca salieron del conjunto de
> fuente. El primero no cambia el comportamiento de los grafos sin colisiones.

## Tasks

| ID | Qué | Specs | Ruta | Commit |
|---|---|---|---|---|
| T1 | Diagnóstico empírico de las colisiones de `order` en el caso `100n/150e + 8 clusters + grupo externo`, sin artefacto enviado | S3, S4 | worker | `eea26f6` |
| T2 | Registro sólo-hoja en `DagreEngine` + cajas ancestro locales (unión + padding, de adentro hacia afuera) | S5, S6 | worker | `ae5b2db` |
| T3 | Red de seguridad (`compound: false` + cajas locales), invariante en test y quinto caso de matriz | S7, S8 | worker | `ae5b2db` |
| T4 | Goldens (sólo `component-nested`), nota en el PRD §8.4 y reporte upstream **redactado, no publicado** | S1, S2, S8 | inline | `03494a3` + `eaa5bdd` |
| T3-bis | Una arista que nombra un grupo hacía panick adentro de `dagre`: el motor la filtra en vez de caerse | S7 | inline | `d935930` |
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
- **L6** (cierre de T2/T3/T4, medido): reproducción de S3 en **100/100** nodos
  después de T2; el disparador que queda es más grande —16 clusters hoja de 8
  nodos con 240 aristas cruzadas: 52 de 128 nodos y 9 de 16 cajas— y es el que
  ejercita la red de seguridad. Goldens: sólo `component-nested` se movió, de
  4170 a 4169 bytes (310.05 × 444.50 contra 338.05 × 479.50, las dos cajas
  anidadas y el nombre interno sangrado 24 unidades). Un caso nuevo: una arista
  que nombra un grupo hacía **panick** en `rank/network_simplex.rs` de dagre; el
  motor ahora la ignora (`d935930`). Gates: `cargo test --workspace` verde,
  clippy sin avisos, fmt limpio.
- **L4** (T1, número medido): reproducción confirmada — 18 de 100 nodos sin
  posición con el grupo externo, 100/100 sin él, 96/96 anidando sin aristas; 4
  celdas `(rank, order)` duplicadas en el resultado y 4 de 18 nodos explicados
  por ellas, el resto por dummies transitorios. El árbol quedó limpio y
  `cargo test -p gx-layout` en verde.
