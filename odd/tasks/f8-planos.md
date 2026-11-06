# F8 — planos por sesión y plano del usuario

Los planos de las sesiones comparten una sola superficie: el colocador ve
todos los rectángulos de todos, una sesión empuja a otra, y ver una versión
vieja más grande tapa al vecino de otra sesión. Este corte separa las
superficies y le da al humano un plano propio para analizar. Sale de la
prueba en vivo del corte `f2-cierre` T5 (2026-11-05).

## Specs

- S1 (pedido original, verbatim): «ok el primer problema, las sesiones tienen
  el mismo plano compartido, por eso es posible ver todos, podemos sacrificar
  ese ver todos, y tener cada sesion en un plano aislado de los otros, al
  final podriamos darle al usuario un plano donde pueda mover diagrama y
  analizar en un plano para el usuario, que este pueda clonar el diagrama y
  verlo ahi, como solucion a el tema del digrama mas grande, podemos
  recalcular su posicion con mas tranquilquidad, sabiendo que las sesiones no
  estan mezcladas».
- S2 (RF30 resuelta, verbatim): «lo remplazamos por un plano para el usuario
  donde el puede copiar sus diagramas a analizar independientemente del
  proyecto».
- S3 (semántica del clon, verbatim): «el snapshot y que este pueda
  actualizarlo si este fue actualizado» — el clon congela el diagrama al
  clonar, y el humano puede traer la última versión del original cuando
  quiera; no es espejo automático.
- S4 (derivados aceptados con «dale»): la vista rápida de versiones
  («◀ v3/7 ▶» del panel, corte `f2-cierre` T5) queda también en los planos de
  sesión; el plano del usuario es donde se navega «cómodo», recalculando
  posición. El clon es **instancia de vista** (estado y posición en el
  dueño): no consume los 12 slots, no tiene `diagram_id`, las tools MCP no lo
  ven, el agente no lo toca. «Actualizar el clon» = re-pedir la versión
  actual del original; si el original se cerró, el clon queda como foto y el
  refresco se deshabilita. Mover objetos es interacción nueva: arrastrar
  fondo = cámara (como hoy), arrastrar objeto = moverlo, y solo en el plano
  del usuario. El contenido del plano del usuario muere al cerrar el proceso
  (como el plano hoy). El conmutador muestra el plano del usuario arriba de
  las sesiones.
- S5 (integración, decisión del 2026-11-05, L3 de `f2-cierre.md`): feature →
  `staging` → PRs encadenados → `main`; nunca directo a `main`; revisión y
  prueba del humano antes del merge. El corte A se apoya sobre la rama de
  `f2-cierre` T5 (`feature/f2-t5-versions-ui`, pendiente de merge).

### Verificación

- V1 (corte A): la colocación de un render colisiona solo con los objetos de
  su propia sesión (test del filtro occupied); el conmutador muestra un plano
  a la vez y «Todas» ya no existe; la cámara es por plano. Suite completa,
  fmt y clippy verdes.
- V2 (corte B): clonar desde el panel crea una instancia de vista con el
  documento del momento; refrescar trae la versión actual del original; mover
  con el puntero escribe solo la posición de la vista; las tools MCP no ven
  clones (test del contrato).
- V3: revisión en vivo del humano antes del merge (S5).

## Tasks

- A1 (S1, V1): colocación por sesión — el occupied del hook de render filtra
  por la sesión del diagrama que entra. Rama: `feature/f8-planos-aislados`.
- A2 (S1, V1): el selector pasa de filtro a conmutador: un plano a la vez,
  «Todas» muere, cámara por plano. Contratos del frontend actualizados.
- A3 (S2, V1): docs — PRD (RF30 y §11.1) y este documento.
- B1 (S2, S4): plano del usuario — modelo en el dueño (instancias de vista),
  conmutador con fila propia, clonar desde el panel.
- B2 (S3, S4, V2): refrescar el clon con la versión actual del original;
  deshabilitado si el original ya no está.
- B3 (S1, S4, V2): mover el clon arrastrando el objeto; navegación de
  versiones en el clon con recálculo de posición.
- B4 (S2, V2): docs — PRD y este documento.

## Log

- L1 (2026-11-05): el humano, tras probar T5 en vivo: «ok el primer problema,
  las sesiones tienen el mismo plano compartido, por eso es posible ver todos,
  podemos sacrificar ese ver todos, y tener cada sesion en un plano aislado de
  los otros, al final podriamos darle al usuario un plano donde pueda mover
  diagrama y analizar en un plano para el usuario, que este pueda clonar el
  diagrama y verlo ahi, como solucion a el tema del digrama mas grande, podemos
  recalcular su posicion con mas tranquilquidad, sabiendo que las sesiones no
  estan mezcladas».
- L2 (2026-11-05): resueltas las dos preguntas del orquestador — RF30: «lo
  remplazamos por un plano para el usuario donde el puede copiar sus diagramas
  a analizar independientemente del proyecto»; clon: «el snapshot y que este
  pueda actualizarlo si este fue actualizado».
- L3 (2026-11-05): «dale» — se abre el corte con A (aislamiento + conmutador)
  primero y B (plano del usuario) después.
