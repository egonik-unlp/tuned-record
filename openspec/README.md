# SDD Flow

Flujo de Spec-Driven Development (SDD) construido sobre [OpenSpec](https://github.com/Fission-AI/OpenSpec)
con capas propias: integración **Graphify** (grafo de conocimiento del código) + sincronización a
`knowledge/` (lore curado) vía **documenter**.

> **Esto NO es OpenSpec vanilla.** El namespace `/opsx:` se mantiene por compatibilidad, pero los
> artefactos, las reglas y el ciclo son específicos de este harness. Si una doc upstream contradice esto,
> manda este README.
>
> **Template.** Este archivo viene de `packages/snappec`. Reemplazá `<package>` por el nombre del
> paquete/repo instrumentado por Graphify en tu proyecto.
>
> **Requiere OpenSpec >= 1.9.0** (`npm i -g @fission-ai/openspec`). Los comandos `/opsx:*` de este
> harness son forks de los templates `opsx` de 1.9.0 y dependen de contratos de CLI que no existen en
> versiones anteriores: `resolvedOutputPath`, `artifactPaths`, `planningHome` y
> `openspec instructions apply|archive --json`. Ver [§ Contratos de CLI](#contratos-de-cli-que-usa-el-flujo).

---

## Ciclo completo

```
/debug (opcional — si el punto de partida es un bug)
       │ termina en un bug report; no genera artefactos
       ▼
/opsx:explore (opcional)
       │ si cierra: entrevista de intención → intent brief → change + context.md
       ▼
/opsx:propose <name>
       │ verifica completitud del input y genera, en orden:
       │   context.md → proposal.md
       │       ⏸ gate de aprobación (Approve / Adjust / Cancel — marca **Estado:** en proposal.md)
       │   → specs.md → design.md → ui-ux.md → tasks.md
       ▼
       │ (opcional, si el grafo cambió antes de apply: /opsx:explore <name> re-consolida context.md)
       ▼
/opsx:apply <name>
       │ implementa las tasks
       ▼
/opsx:archive <name>
       │ valida (bloqueante) + sincroniza delta specs → openspec/specs/ (vía /opsx:sync, inline)
       │ + mueve a archive/YYYY-MM-DD-<name>/
       │ + auto-invoca /documenter distill <dated-change-id>
       ▼
knowledge/modules/<module>/use-cases/<capability>/   ← sincronizado (módulo resuelto por glob)
```

---

## Los 6 artefactos

Cada change vive en `openspec/changes/<name>/`. Se generan en orden — cada uno depende del anterior.
Definición canónica en [schemas/sdd-with-knowledge/schema.yaml](schemas/sdd-with-knowledge/schema.yaml).

| # | Artefacto | Propósito | Requires |
|---|---|---|---|
| 1 | `context.md` | Subgrafo Graphify + archivos afectados + knowledge aplicable + riesgos. **Base de los otros 5.** | — |
| 2 | `proposal.md` | Why / Scope / Impact / Non-goals. Referencia nodos del grafo (`graph://node:X`). | `context` |
| 3 | `specs/<capability-path>/spec.md` | Delta spec `ADDED`/`MODIFIED`/`REMOVED Requirements`. Cada uno con ≥1 scenario Given/When/Then. | `proposal` |
| 4 | `design.md` | Approach técnico. Decisiones con rationale — `distill` las lleva a `knowledge/` post-archive. | `specs` |
| 5 | `ui-ux.md` | Primera línea = veredicto: `Aplica` o `No aplica — backend-only` + justificación. | `design` |
| 6 | `tasks.md` | Checklist. Última task: `/documenter distill <dated-change-id>` (auto-ejecutada por `archive`). | `ui-ux` |

---

## Los comandos

Viven en [.claude/commands/opsx/](../.claude/commands/opsx/). Son **4 comandos de flujo** (los que
invocás a mano) + **1 subrutina** (`sync`, que `archive` corre inline).

| Comando | Cuándo | Qué hace |
|---|---|---|
| `/opsx:explore` | Antes de proponer, si no sabés por dónde empezar | Thinking partner divergente. No genera artefactos por iniciativa propia; si el usuario pide cerrar, hace la entrevista de intención y **siempre** deja el change abierto con `context.md` (entrevista registrada) — nada más. |
| `/opsx:explore <name>` | Para iterar el `context.md` de un change existente | Único escritor de `context.md`: extiende la entrevista (append) y/o re-consolida el snapshot de grafo/knowledge. Diff + confirmación antes de escribir; no toca los otros 5. |
| `/opsx:propose <name>` | Para abrir un change | Verifica completitud del input (brief completo = cero preguntas; huecos = una pregunta por hueco), crea el change y genera los 6 artefactos en orden, con **una única pausa**: el gate de aprobación del proposal (`Approve`/`Adjust`/`Cancel`) antes de los artefactos que dependen de él. |
| `/opsx:apply <name>` | Para implementar | Ejecuta las tasks, leyendo `context.md` y `design.md`. |
| `/opsx:archive <name>` | Cuando el change está listo | Valida (**bloqueante**), sincroniza specs, mueve a archive (con fecha) y **auto-invoca `/documenter distill <dated-id>`**. |
| `/opsx:sync <name>` | Normalmente **no a mano** | Merge inteligente de las delta specs → `openspec/specs/`. `archive` lo corre inline en su paso 4. |

**Comandos de vanilla 1.9.0 que este harness NO instala:** `new`, `continue`, `update`, `ff`,
`bulk-archive`, `verify`, `onboard`. `propose` cubre `new` + `continue` (loopea artefactos hasta que el
change está apply-ready), así que si `/opsx:apply` reporta `blocked`, re-corré `/opsx:propose <name>`.

---

## Comando auxiliar: `/debug`

Vive en [.claude/commands/debug.md](../.claude/commands/debug.md). Está **fuera del namespace `/opsx:`**: no
genera artefactos, no escribe en `openspec/` y **no cuenta como un 6º comando del flujo**. Es el punto de
entrada cuando el trabajo arranca en un **bug** y no en una feature — reemplaza el "contame qué pasa" informal
por un interrogatorio socrático que termina en un bug report verificado.

| Fase | Qué pasa |
|---|---|
| 1. Understand | Preguntas al usuario **antes** de leer código: comportamiento esperado vs. observado, stack trace / valor literal, qué entidad falla y en qué estado, escenario mínimo de reproducción, entorno, timing y alcance, hipótesis ya descartadas. Se pregunta de nuevo mientras queden huecos, y cierra con una **hipótesis explícita** validada con el usuario desde el negocio. |
| 2. Investigate | Recién acá se lee código, **guiado por la hipótesis** (no exploración abierta). Si el código la contradice, se revisa la hipótesis — no se fuerza la explicación. Root cause con `<file>:<line>`, condición gatillo y downstream. |
| 3. Verify | Checklist de evidencia: ¿se reproduce con datos concretos?, ¿el root cause explica el **100%** del síntoma?, ¿hay otras rutas al mismo síntoma?, ¿el fix rompe flujos adyacentes (rollback, retries, edge cases)? Lo que no se pudo verificar se marca como **incertidumbre abierta**. |

**Output final** (bloque fijo — sirve como input de `/opsx:propose` o como descripción del PR/issue):

```
## Bug report

**Symptom:** <lo que ve el usuario>
**Root cause:** <file:line — explicación>
**Reproduction condition:** <combinación exacta de datos/estado>
**Downstream:** <dónde se rompe visiblemente>
**Flows that must NOT break:** <rollback, retries, integraciones…>
**Open uncertainties:** <lo que no se pudo verificar>
```

**Cierre duro — no se pasa al fix sin confirmación.** `/debug` **termina en el bug report**, aunque el root
cause esté claro y el fix parezca de una línea. Frases como *"hay que arreglar X"*, *"solo falta cambiar Y"* o
*"el fix es trivial"* describen la **dirección** del fix, no autorizan a aplicarlo: antes de tocar código el
comando pregunta si se aplica ahí mismo o si el bug report entra al flujo SDD vía `/opsx:propose`. Saltear el
flujo deja el cambio sin change registrado en `openspec/`, sin artefactos y sin trazabilidad — para fixes de
baja fricción puede tener sentido, pero **es decisión del humano, no del agente**.

**Camino natural:** `/debug` → bug report → `/opsx:propose fix-<algo>` (el slug lleva prefijo `fix-`; pegá el
bug report como input) → `apply` → `archive` → `distill`.

**`/debug` vs `/opsx:explore`:** `debug` parte de un **síntoma concreto** y converge a un root cause
verificado; `explore` parte de una **idea o feature** y abre el espacio de soluciones. Si el bug ya está
entendido y lo que falta es decidir el approach, usá `explore`. Si el síntoma todavía es difuso, `debug`
primero.

---

## Navegación: 3 capas

`knowledge/` se organiza en tres capas, de lo general a lo específico:

```
knowledge/INDEX.md                              # 1. índice de módulos (entry point)
knowledge/modules/<module>/INDEX.md             # 2. navegación del módulo: tabla "¿Qué quiero hacer?" (intent → flow)
knowledge/modules/<module>/use-cases/<cap>/     # 3. contenido del use-case (spec/flow/decisions/notes)
```

El **módulo** es la unidad de navegación/dominio (agrupa N use-cases, con diagrama de alto nivel y notas
transversales); el **use-case** es la unidad de contrato/cambio.

## Convención capability ↔ use-case

**Regla 1:1**: el slug en `openspec/specs/<capability>/` coincide con el use-case **anidado**
`knowledge/modules/<module>/use-cases/<capability>/`. El slug es **único globalmente** (lo garantiza
`openspec/specs/` plano), así que `distill` resuelve la ubicación por **glob**
`knowledge/modules/*/use-cases/<capability>/` (0 = nueva, 1 = existente, >1 = error). La **ubicación es la
membresía** — no hay frontmatter `module:`; mover de módulo es `git mv`. Los **módulos** no tienen slug en
`openspec/specs/` y no sincronizan por nombre; solo los use-cases dentro lo hacen.

### Archivos por use-case en `knowledge/`

Cada `knowledge/modules/<module>/use-cases/<capability>/` agrupa cuatro archivos, separados por **propósito**
(no por estructura de código — eso lo responde Graphify):

| Archivo | Responde | Modo |
|---|---|---|
| `spec.md` | **QUÉ** debe cumplirse (contrato narrativo, sync con `openspec/specs/`) | reference-ish |
| `flow.md` | **CÓMO** funciona end-to-end y **POR QUÉ** del comportamiento (flujo funcional + diagrama Mermaid) | explanation |
| `decisions.md` | **POR QUÉ** se eligió esta solución sobre alternativas (opcional) | explanation |
| `notes.md` | lore, gotchas, TODOs no derivables | — |

> **`flow.md` NO es un 7º artefacto del change.** Los 6 artefactos de `/opsx:propose` son fijos. `flow.md` es un
> destino de `knowledge/` que `documenter distill` **deriva** post-archive (de `proposal`/`design`/`specs`/`ui-ux`),
> traducido a lenguaje funcional. Es **obligatorio** y lleva veredicto en su 1ª línea (`Aplica` / `No aplica`).
> La prosa es la fuente de verdad; el diagrama Mermaid es una vista derivada. Detalle en
> [.claude/skills/documenter/SKILL.md](../.claude/skills/documenter/SKILL.md).

---

## Integración Graphify

El grafo del código vive en `<package>/graphify-out/` y se reconstruye automáticamente vía git hooks
(`.githooks/post-commit`, `.githooks/post-checkout` — instalados por el setup del harness).

**Cómo consultarlo**:
1. **Preferido**: Graphify MCP, tools `query_graph`, `get_node`, `get_neighbors`, `shortest_path`.
2. **Fallback**: leer `<package>/graphify-out/GRAPH_REPORT.md`.
3. **Código directo**: solo si lo anterior no alcanza, scoped al subgrafo.

**Setup**: el harness deja el MCP configurado en `.mcp.json` (server `graphify` → `scripts/graphify-mcp.sh`,
que resuelve el intérprete en runtime). Requiere `graphifyy` instalado y el grafo construido; si falta,
se usa el fallback `GRAPH_REPORT.md`.

---

## Integración documenter

[.claude/skills/documenter/SKILL.md](../.claude/skills/documenter/SKILL.md) mantiene `knowledge/`:

| Subcomando | Cuándo | Qué hace |
|---|---|---|
| `/documenter bootstrap` | Al inicializar el repo | Crea el árbol de `knowledge/` (INDEX global + `modules/`; siembra módulos desde comunidades Graphify si están). |
| `/documenter update` | Manual, ante sospecha de drift | Detecta divergencia vs git log, propone edits. Humano en el loop. |
| `/documenter distill <id>` | Auto post-archive; manual para re-runs | Sincroniza specs/decisiones/lore al use-case (`knowledge/modules/<m>/use-cases/<capability>/`, resuelto por glob) + su `INDEX.md` de módulo. |

---

## Contratos de CLI que usa el flujo

Los comandos `/opsx:*` **no hardcodean paths ni el orden de los artefactos**: los leen del CLI. Esta es la
superficie de la que dependen (OpenSpec >= 1.9.0). Si actualizás OpenSpec y algo se rompe, empezá acá.

| Comando de CLI | Campos que consumimos | Quién lo usa |
|---|---|---|
| `openspec status --change <n> --json` | `schemaName`, `applyRequires`, `artifacts[].status` + `.requires`, `planningHome`, `changeRoot`, `artifactPaths.<id>.existingOutputPaths`, `actionContext` | los 5 comandos |
| `openspec instructions <artifact> --change <n> --json` | `template`, `instruction`, `context`, `rules`, `resolvedOutputPath`, `dependencies` | `propose`, `explore`, `sync` |
| `openspec instructions apply --change <n> --json` | `contextFiles`, `progress`, `tasks`, `state`, `context`, `operationGuidance` | `apply` |
| `openspec instructions archive --change <n> --json` | `context`, `operationGuidance` | `archive` |
| `openspec new change <n>` | scaffold + `.openspec.yaml` | `propose`, `explore` |
| `openspec validate <n> --type change --strict` | exit code | `archive` (gate bloqueante, paso 4) |
| `openspec validate --specs` | exit code | `sync` |

**Nunca construir paths a mano.** `planningHome` / `changeRoot` / `artifactPaths` existen porque un change
puede vivir en un *store* (repo OpenSpec standalone registrado en la máquina) y no en este repo. Asumir
`openspec/changes/<name>/` rompe ese caso.

### Dónde vive la lógica de proyecto

| Canal | Qué pone | Lo lee |
|---|---|---|
| `config.yaml` → `context:` | Background del proyecto (Graphify, knowledge, regla divisoria) | todos los artefactos + `apply` + `archive` |
| `config.yaml` → `rules:` | Reglas por artefacto (`context`, `proposal`, `specs`, `design`, `ui-ux`, `tasks`) | el artefacto correspondiente |
| `config.yaml` → `operations:` | Guidance por operación (`apply`, `archive`) — acá vive el auto-distill | `/opsx:apply`, `/opsx:archive` |
| `schemas/sdd-with-knowledge/` | Artefactos, orden, dependencias, `instruction` y `templates/` | el CLI, vía `status` / `instructions` |

`context` y `rules` son **constraints para el agente, nunca contenido del artefacto** — no se copian al
archivo generado. Lo mismo aplica a `operationGuidance`.

⚠️ **No duplicar lógica de proyecto en el markdown de los comandos.** Los `/opsx:*` son forks mínimos de
vanilla; cada línea propia es drift que hay que re-aplicar en el próximo upgrade. Si algo se puede expresar
en `config.yaml` o en el schema, va ahí. Las excepciones vigentes están marcadas con
`⚠️ LOCAL CUSTOMIZATION` en el markdown y listadas en [CONVENTIONS.md](CONVENTIONS.md).

---

## Cosas que NO van acá

- **Estructura del código** (entry points, dependencias) → Graphify. No documentar a mano.
- **Git history** → `git log` / archive.
- **State in-progress** → `openspec/changes/<name>/` mientras existe.
- **Lore funcional permanente** → `knowledge/`. No en `openspec/specs/` (esos son contratos formales).

---

## Referencias

- Comando de debugging: [../.claude/commands/debug.md](../.claude/commands/debug.md)
- Schema: [schemas/sdd-with-knowledge/schema.yaml](schemas/sdd-with-knowledge/schema.yaml)
- Config con context+rules: [config.yaml](config.yaml)
- Reglas duras y edge cases: [CONVENTIONS.md](CONVENTIONS.md)
- Setup e instalación del harness: [../README.md](../README.md)

**Gate de CI:** `openspec validate --changes --strict` (o `--all --strict`). Desde 1.9.0 el `--strict`
pelado **no valida nada** y sale con exit 1 — necesita un flag de scope explícito.
