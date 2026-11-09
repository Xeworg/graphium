# Integraciones de Graphium para clientes MCP

Graphium habla MCP por stdio: cualquier cliente que sepa configurar un servidor
lo puede usar escribiendo a mano el comando y el `--stdio`. Esta carpeta existe
para lo que viene **después** de eso: la integración propia de cada cliente, con
lo que el protocolo no cubre —encontrar el binario, no pedir configuración a
mano, y un comando para el humano.

| Cliente | Estado | Qué agrega |
|---|---|---|
| [Pi](pi/graphium/README.md) | ✅ | Registra el servidor MCP solo, lo describe con el libro a la vista y agrega `/graphium` (estado y `open`) |

Cada integración es independiente: no comparte código con el workspace de Rust y
no cambia nada del binario. Si una se rompe, las otras siguen.

## La regla de la casa

Una integración **no reimplementa** el motor ni habla el protocolo por su cuenta:
el cliente conecta el binario por MCP y las tools son las mismas que tiene
cualquier otro. Lo que la integración aporta es el camino corto hasta ahí, y
nunca guarda estado del plano: el plano es del humano y vive en la ventana.
