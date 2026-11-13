# Los lenguajes del libro

Veinte lenguajes viajan en el binario y el libro indexa los archivos de cada uno
que encuentre en el proyecto. Este documento publica una sola cosa por lenguaje:
**qué parte de lo que el capítulo declara el libro puede resolver**, con el
número medido al lado del fixture donde se midió y del test que lo pincha. No es
una tabla de popularidad ni una promesa sobre un repositorio real.

## Qué mide el porcentaje

`percent = resolved / edges × 100`, calculado en
`crates/gx-index/src/resolution.rs` y publicado por `code_index` en
`languages[].resolution { edges, resolved, percent, comparable }`:

- **Denominador:** cada arista que el capítulo declaró y que se puede atar a un
  archivo (la arista tiene que nacer en un símbolo del libro).
- **Numerador:** las aristas cuyo destino tiene un nombre que el libro puede
  desambiguar: **hasta cinco definidores** en todo el libro (`MAX_NAME_FANOUT`).
  Un nombre que seis capítulos declaran no se resuelve, porque elegir uno sería
  inventar.
- **Fuera de las dos cuentas:** los destinos que el runtime o la biblioteca
  estándar del lenguaje proveen (`str`, `len`, `String`), **pero solo en los
  lenguajes que traen una lista de nombres**. Esa exclusión es la que hace que
  dos tasas no sean siempre comparables.

No es un porcentaje de archivos, ni de símbolos, ni de nodos del AST: es de
**aristas**, y una arista es una relación que el capítulo declaró (un `import`,
una llamada, una herencia).

### `comparable`: cuándo dos tasas se pueden leer juntas

Una lista de nombres de runtime hace que el denominador de ese lenguaje sea más
angosto que el de uno que no la tiene (le saca los destinos imposibles), así que
comparar una fila con lista contra una sin lista es comparar dos cosas
distintas. Por eso cada fila lleva `comparable`, y es `true` solo para un
lenguaje con lista no vacía. **Ocho** lenguajes la tienen —`python`, `go`,
`java`, `swift`, `kotlin`, `dart`, `scala`, `r`—; los otros doce cuentan sus
llamadas a la biblioteca estándar como no resueltas, y por eso su tasa es más
baja por construcción, no por peor gramática.

Regla de lectura: **dos tasas se comparan solo si las dos filas dicen
`comparable: true`**. La respuesta de `code_index` lleva el flag justamente para
que esa regla viaje con el número en vez de vivir en este párrafo.

## La tabla

Medida el 2026-11-12 en el worktree de `f28-lenguajes`, sobre los fixtures de
`crates/gx-index/tests/languages.rs`: cada fila es el libro de un proyecto de
prueba de una o dos archivos, no un repositorio real. El **piso** es el número
publicado que el test exige: si una medición cae por debajo, la suite falla
(D5: un piso baja solo con la medición escrita al lado).

| Lenguaje | Archivos | Símbolos | Aristas | Resueltas | Tasa | `comparable` | Piso | Test |
|---|---:|---:|---:|---:|---:|:---:|---:|---|
| `c` | 1 | 10 | 11 | 9 | 81,8 % | no | 79,0 % | batch 1 |
| `cpp` | 1 | 14 | 14 | 10 | 71,4 % | no | 69,0 % | batch 1 |
| `csharp` | 1 | 16 | 15 | 10 | 66,7 % | no | 64,0 % | batch 1 |
| `dart` | 1 | 16 | 13 | 7 | 53,8 % | sí | 53,0 % | batch 3 |
| `go` | 1 | 14 | 10 | 5 | 50,0 % | sí | 46,0 % | T6 |
| `java` | 1 | 14 | 12 | 8 | 66,7 % | sí | 63,0 % | T6 |
| `javascript` | 1 | 12 | 12 | 8 | 66,7 % | no | 64,0 % | batch 1 |
| `kotlin` | 1 | 16 | 14 | 7 | 50,0 % | sí | 50,0 % | batch 3 |
| `lua` | 2 | 10 | 5 | 4 | 80,0 % | no | 76,0 % | batch 4 |
| `php` | 1 | 19 | 20 | 19 | 95,0 % | no | 91,0 % | batch 2 |
| `python` | 1 | 11 | 10 | 4 | 40,0 % | sí | 36,0 % | T6 |
| `r` | 2 | 7 | 5 | 4 | 80,0 % | sí | 80,0 % | batch 4 |
| `ruby` | 1 | 14 | 9 | 6 | 66,7 % | no | 63,0 % | batch 2 |
| `rust` | 1 | 9 | 11 | 4 | 36,4 % | no | — | T6 |
| `scala` | 2 | 20 | 18 | 16 | 88,9 % | sí | 88,0 % | batch 4 |
| `shell` | 2 | 9 | 8 | 3 | 37,5 % | no | 34,0 % | batch 2 |
| `sql` | 1 | 6 | 4 | 4 | 100,0 % | no | 96,0 % | batch 2 |
| `swift` | 1 | 20 | 12 | 7 | 58,3 % | sí | 58,0 % | batch 3 |
| `typescript` | 2 | 20 | 25 | 12 | 48,0 % | no | 45,0 % | T6 |
| `zig` | 1 | 10 | 8 | 7 | 87,5 % | no | 83,0 % | batch 4 |

`rust` no tiene piso: el test lo mide y exige que tenga capítulos, símbolos y
aristas, pero no fija su tasa. Un 100 % en una fila no dice «este lenguaje es
perfecto»: dice que en este fixture todas las relaciones declaradas apuntaban a
nombres que el libro pudo desambiguar.

## Dónde vive cada número

Cinco tests de `crates/gx-index/tests/languages.rs` miden los veinte, y cada uno
imprime su línea `D3 resolutions …` al correr con `--nocapture`:

| Test | Lenguajes |
|---|---|
| `the_resolution_rate_of_each_language_is_measured_on_the_fixtures` | `typescript`, `python`, `go`, `java`, `rust` |
| `the_resolution_rate_of_the_batch_one_languages_is_measured_on_their_fixtures` | `javascript`, `c`, `cpp`, `csharp` |
| `the_resolution_rate_of_the_batch_two_languages_is_measured_on_their_fixtures` | `php`, `ruby`, `shell`, `sql` |
| `the_resolution_rate_of_the_batch_three_languages_is_measured_on_their_fixtures` | `swift`, `kotlin`, `dart` |
| `the_resolution_rate_of_the_batch_four_languages_is_measured_on_their_fixtures` | `scala`, `r`, `lua`, `zig` |

Comando: `cargo test -p gx-index --test languages -- --nocapture`. Cada test
mide su propio libro: una tanda no puede mover el número de otra.

El censo que un agente consulta en vivo es el de `code_index`
(`files`, `failed[].reason`, `symbols`, `languages[].resolution` y los `limits`
en vigor), con los mismos cuatro códigos de razón para un archivo que no entró al
libro: `ignored`, `too_large`, `unsupported_language`, `unreadable`.

## Dos reducciones que se cobraron en `f28-lenguajes`

- **Go: el import por ruta de paquete.** Un capítulo Go declara el paquete que es
  (símbolo `module` con el nombre del `package_clause` y el module path del
  directorio), así que un import de paquete resuelve contra esa declaración en
  vez de contra un nombre suelto. En el fixture de dos archivos la tasa pasa de
  **5/6 = 83,3 % a 6/6 = 100,0 %**. En la tabla no se mueve nada: el fixture de
  este documento es de un solo archivo y no importa su propio paquete, y el
  libro-wide suma un definidor por directorio (riesgo de fanout que en los
  fixtures no se movió).
- **Dart: la mixin application es una declaración.** `class C = Base with M;`
  parsea como `class_declaration → mixin_application_class` y ese nodo no tiene
  campo `name`, así que el símbolo no se nombraba y sus miembros no se visitaban.
  Desde `f28-lenguajes` el nodo se nombra por el `identifier` de la mixin
  application y la clase queda declarada (con sus `with` colgando de ella). **La
  ortografía `mixin C = Base with M;` no es Dart**: el grammar la recupera como
  una declaración de variable de nivel superior (`const C` de tipo `mixin`), y
  ninguna prosa de este repositorio debe escribirla como si fuera la forma del
  lenguaje. En el fixture del piso la tasa no cambia (7/13 = 53,8 % antes y
  después): la reducción compra la declaración, no el porcentaje.

## El binario

Los veinte lenguajes viajan en el binario por defecto —cada gramática sigue
siendo una feature, todas encendidas—, así que el tamaño del release es una
deuda de este documento. Medido el 2026-11-12 en el worktree de
`f28-lenguajes`, con `cargo build --release` y `du -b`:

| Build | Bytes | Sobre f10 |
|---|---|---|
| default (los veinte lenguajes) | **46.203.544 B** | **+397.312 B** |
| flaco (`lang-rust,lang-typescript`) | **13.618.328 B** | **+376.832 B** |

El default es el **68,8 %** de la puerta de **67.108.864 B** (64 MiB), que deja
**20.905.320 B (31,1 %)** de aire; el flaco **ya no es byte-idéntico** al de f10
(13.241.496 B), porque `gx-index` creció para los dos builds. La puerta ya no
es un plan de crecimiento: con el techo en 64 MB —decisión del owner del
2026-11-12, que reemplaza el techo de 48 MB y la puerta de 50 300 000 B— es una
**alarma de accidente**, y solo salta si una dependencia o un asset agrega
decenas de megabytes. El presupuesto y su contrato viven en `docs/PRD.md`
RNF1/RNF1-quater, y el gate que lo hace fallar está en
`.github/workflows/ci.yml`; este documento fija la medición del corte.

## Cómo se mantiene

Un lenguaje nuevo entra con su módulo, su feature, su fixture y su piso en el
mismo commit; su fila entra en esta tabla con la medición que la respalda. Un
piso que se mueve, se mueve con la medición escrita al lado: nunca por
redondeo, nunca «para que pase».
