#!/usr/bin/env node
// Prints a Graphium diagram document (the `render_diagram` argument) with `n`
// nodes and `n - 1` edges, built programmatically. Used by `bench-live.sh`, so
// the small and the large fixtures of the bench are the same shape for both
// frontends.
//
//   node tools/diagram.mjs 12  > /tmp/small.json
//   node tools/diagram.mjs 400 > /tmp/large.json

const count = Number(process.argv[2] || 12);
if (!Number.isInteger(count) || count < 2) {
  console.error("diagram: usage: node tools/diagram.mjs <nodes >= 2>");
  process.exit(2);
}

const nodes = [];
const edges = [];
for (let index = 0; index < count; index += 1) {
  nodes.push({ id: `n${index}`, label: `nodo ${index}` });
  if (index > 0) {
    edges.push({ from: `n${index - 1}`, to: `n${index}`, label: index % 5 === 0 ? `paso ${index}` : "" });
  }
}

const document = {
  diagram: {
    version: 1,
    kind: "graph",
    title: `bench ${count}`,
    direction: "TB",
    nodes,
    edges,
  },
};

process.stdout.write(`${JSON.stringify(document)}\n`);
