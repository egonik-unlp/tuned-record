# SDD Conventions

Reglas duras del flujo. Para el flujo de punta a punta, leer primero [README.md](README.md). Este
documento resuelve edge cases y deja explícitas las decisiones de diseño.

> **Template.** Este archivo viene del harness `packages/snappec`. Los ejemplos son genéricos;
> adaptá los nombres a tu repo a medida que aparezcan capabilities reales.

---

## Naming

### Slug del change
- Formato: `kebab-case`.
- Prefijo verbo: `add-`, `replace-`, `remove-`, `refactor-`, `update-`, `fix-`.
- **No** se escribe la fecha a mano. `archive` agrega el prefijo `YYYY-MM-DD-` al mover a
  `openspec/changes/archive/`. Ejemplo: `add-foo` → archivado como `2026-05-26-add-foo`.

### Slug del capability
- Formato: `kebab-case`, sustantivo singular o frase corta (`user-auth`, `email-notifications`).
- **Regla 1:1**: el slug en `openspec/specs/<capability>/` = nombre exacto de la carpeta del use-case, que
  vive **anidado bajo un módulo**: `knowledge/modules/<module>/use-cases/<capability>/`. El slug es **único
  globalmente** (lo garantiza `openspec/specs/` plano), así que `/documenter distill` resuelve la ubicación
  por **glob** `knowledge/modules/*/use-cases/<capability>/` (0 hits = nueva; 1 = existente, in situ; >1 =
  error). La **ubicación es la membresía** — no hay frontmatter `module:`.
- Los **módulos** (`knowledge/modules/<module>/`) son capa de navegación: NO tienen slug en
  `openspec/specs/` y NO sincronizan por nombre. Solo los use-cases dentro de ellos lo hacen.
- El módulo reservado `_unsorted` aloja capabilities nuevas sin módulo resuelto; se mueven a su módulo
  definitivo con `git mv` (distill las re-encuentra por glob).
- Si tu repo tiene use-cases legacy sin spec viva en `openspec/specs/`, respetalos como historia. La regla
  1:1 aplica a capabilities **nuevas**, no retroactivamente.

---

## Validación pre-archive

`/opsx:archive` ejecuta, en orden (pasos 2–4 del comando):

1. Chequeo de artefactos (`openspec status --json`): cada uno en `done`/`skipped` — warning si no, no
   bloquea, se confirma al usuario.
2. Chequeo de `tasks.md`: warning si quedan tasks `- [ ]` abiertas — se confirma al usuario.
3. `openspec validate <change-name> --type change --strict` — **bloqueante**: si falla, no se sincroniza
   ni se archiva.
4. Para cada capability en delta specs: glob `knowledge/modules/*/use-cases/<capability>/` — 0 o 1 hit OK
   (0 = la creará `distill`, que la ubica en un módulo); >1 = colisión de slug (warning fuerte).
5. La **última task** en `tasks.md` es `post-archive: /documenter distill <dated-change-id>`, anotada como
   **auto-ejecutada por `/opsx:archive`** — warning si falta. Es recordatorio/fallback: el trigger real
   vive en `archive`. El re-run manual usa el id completo con fecha.
6. Marca de aprobación del proposal: warning si el `proposal.md` del change no dice `**Estado:** Aprobado`
   (o no tiene la línea — changes anteriores al gate no la tienen). **Advisory, nunca bloquea.**

---

## Multi-capability changes

Un change puede tocar más de una capability:
- Un solo `proposal.md`, `design.md`, `ui-ux.md`, `tasks.md`.
- En `specs/`, carpetas separadas: `openspec/changes/<name>/specs/<capability-A>/`, `.../<capability-B>/`.
- El slug del change describe el **cambio transversal**, no una capability puntual.
- `archive` valida y sincroniza cada capability por separado contra su use-case (resuelto por glob
  `knowledge/modules/*/use-cases/<capability>/`).

---

## Deprecaciones (REMOVED Requirements)

- `openspec/specs/<capability>/spec.md` se actualiza eliminando el requirement.
- `knowledge/modules/<module>/use-cases/<capability>/spec.md` **no se borra automáticamente**:
  - Si todo el use-case queda deprecado, marcar `status: deprecated` en frontmatter y dejar el contenido
    como histórico.
  - Si solo se deprecan requirements parciales, dejar la spec narrativa cubriendo el comportamiento actual.
- Razón: `knowledge/` es contexto consultable; borrar a la primera deprecación pierde historia útil.

---

## Versionado de specs

**No hay semver de capabilities.** El git history de `openspec/changes/archive/` es el versionado: cada
archive con fecha es un punto en la línea de tiempo de la capability. Es un non-goal explícito; se reabre
solo si hace falta consumir specs versionadas desde otro repo.

---

## Edge cases

### Change abortado a mitad de `apply`
- Borrar `openspec/changes/<name>/` a mano (`rm -rf`). **No usar `archive`** para "deshacer".

### Re-run de `/documenter distill`
- El change tiene que estar **archivado**. Comando: `/documenter distill <dated-change-id>` (nombre completo
  con fecha). Útil si rechazaste edits la primera vez.

### Re-run de `/opsx:propose` sobre un change existente
- No regenera artefactos existentes: pregunta si continuar el change y crea **solo los faltantes** (el
  status del CLI es existencia de archivo). Es el recovery path oficial cuando `/opsx:apply` reporta
  `blocked`.
- Respeta el `context.md` existente (lee "Entrevista de intención" como fuente), relee `context.md` y
  `proposal.md` de disco y **pregunta solo deltas**. Re-gatea el proposal **solo si la marca no está
  `Aprobado`**: con `**Estado:** Aprobado — <fecha>` la construcción sigue directa, sin re-preguntar.
- **Única excepción de regeneración**: dentro del gate de aprobación, un **Adjust** pedido explícitamente
  por el usuario regenera `proposal.md` (y esa regeneración resetea la marca a `Pendiente de aprobación`).
  Fuera del gate, el re-run sigue siendo aditivo y no pisa nada.
- Para regenerar cualquier otro artefacto de verdad, borralo antes (o editalo a mano). Si solo querés
  refrescar el contexto, usar el siguiente.

### Change cancelado en el gate de aprobación
- **Cancel** en el gate deja el change en disco con `context.md` + `proposal.md` escritos y la marca en
  `Pendiente de aprobación`; no se genera nada más. No es un estado roto: re-correr
  `/opsx:propose <name>` re-presenta el gate y retoma desde ahí. Para descartarlo del todo, borrar
  `openspec/changes/<name>/` a mano (igual que un change abortado).

### Marca de aprobación vieja por edición manual
- Una edición manual de `proposal.md` **posterior** al Approve deja la marca `Aprobado — <fecha>` vieja:
  la marca es **autoritativa** (se aprueba un texto, no un archivo, pero nadie la resetea por vos).
  Límite conocido y aceptado. Bajarla a `Pendiente de aprobación` es manual — editando la línea — o
  re-corriendo el gate tras un Adjust que regenere el archivo.

### Iterar `context.md` sin perder el resto
- `/opsx:explore <change-name>` — el único camino para reescribir `context.md`. Sirve para consolidar un
  snapshot pendiente, refrescar el grafo/knowledge cuando se movieron, o seguir la entrevista.
- La sección **"Entrevista de intención"** crece por **append**: Q&A nuevas al final, las previas nunca se
  reescriben, reordenan ni borran; el intent brief se actualiza con respuestas reales, nunca se
  re-sintetiza de la conversación. Un `context.md` pre-schema-v4 no la tiene y la iteración no falla ni la
  inventa.
- Las 4 secciones derivadas (Nodos afectados / Archivos relevantes / Contexto knowledge / Riesgos
  identificados) se regeneran completas; los marcadores `_Pendiente_` se reemplazan por contenido real.
- Semilla de alcance: `proposal.md` si existe, si no el intent brief. Sin ninguna de las dos, se pregunta.
- Siempre **diff + confirmación** antes de escribir. Conserva `proposal/specs/design/ui-ux/tasks`: si el
  alcance se movió, revisarlos aparte.

### Falla de auto-distill en `archive`
- El archive **igual completa** (move + sync de specs). Re-correr: `/documenter distill <dated-change-id>`.
- "Nothing to distill" en changes puramente operacionales (tooling/config) está OK.

---

## State queries durante el flujo

Se usa OpenSpec upstream directo desde CLI — no hay comandos custom:
- `openspec list` — changes activos.
- `openspec status --change <name> --json` — estado de artefactos.
- `openspec validate <name>` — validar (lo que corre `archive`).

---

## Commands del documenter

Set formal: `bootstrap`, `update`, `distill <change-id>`. Todos vía `/documenter <sub>`.
- `bootstrap` — una sola vez al inicializar un repo.
- `update` — a discreción cuando se sospecha drift.
- `distill` — auto desde `archive`; manual solo para re-runs.

No agregar más subcomandos sin discutir — el set está pensado para mantener el ciclo simple.

---

## Comando `/debug` (fuera de `/opsx:`)

`/debug` (`.claude/commands/debug.md`) es el punto de entrada cuando el trabajo arranca en un **bug**. No
pertenece al set `/opsx:` ni lo extiende: no genera artefactos y no escribe en `openspec/`. Reglas duras:

- **Preguntas antes que código.** La fase *Understand* se cierra con una **hipótesis explícita** validada por
  el usuario; recién entonces se lee código, y guiado por esa hipótesis (no exploración abierta). Si el código
  la contradice, se revisa la hipótesis.
- **Evidencia antes de conclusión.** El root cause se reporta con `<file>:<line>` + condición gatillo +
  downstream, y tiene que explicar el **100%** del síntoma. Lo no verificado se declara como *incertidumbre
  abierta*, no se omite.
- **Termina en el bug report.** Ese bloque (`Symptom` / `Root cause` / `Reproduction condition` / `Downstream`
  / `Flows that must NOT break` / `Open uncertainties`) es el entregable. **No se editan archivos en la misma
  sesión.**
- **El fix no se aplica sin autorización explícita.** *"Hay que arreglar X"* o *"el fix es trivial"* describen
  la dirección, no autorizan. Se pregunta: ¿fix directo acá, o el bug report entra al flujo vía
  `/opsx:propose`? Saltear el flujo (change sin registrar, sin artefactos, sin trazabilidad) es decisión del
  humano.
- **Camino esperado:** `/debug` → bug report → `/opsx:propose fix-<algo>` (prefijo verbo `fix-`, ver Naming).

Diferencia con `/opsx:explore`: `debug` parte de un síntoma concreto y converge a un root cause verificado;
`explore` parte de una idea/feature y abre el espacio de soluciones.

---

## Decisiones cerradas (registro)

| # | Tema | Resolución |
|---|---|---|
| 1 | Rol de `/opsx:explore` | Opcional, recomendado para changes grandes. Divergente por default: **no genera artefactos por iniciativa propia**. Tiene un **cierre convergente** — a pedido del usuario (o a oferta del agente, aceptada): entrevista de intención (intención / alcance / comportamiento observable / non-goals / criterios de aceptación, preguntas de a una o en grupos naturales, cierre auto-evaluado) → **intent brief** como bloque fijo en el chat, input canónico de `/opsx:propose`. La instancia de pregunta-respuesta es **ineludible**: el brief nunca se sintetiza solo de la conversación divergente (mínimo una ronda de confirmación). El cierre **siempre persiste** la entrevista: `openspec new change` + **solo `context.md`** con la sección "Entrevista de intención" completa. No se pide autorización para guardar — solo se confirma el nombre; un brief que queda en el chat se pierde. El **snapshot técnico** (nodos / archivos / knowledge / riesgos) es **opcional y recomendado**: si se declina, esas 4 secciones quedan con marcador `_Pendiente — correr /opsx:explore <name>_`. explore es además el **dueño único de `context.md`**: lo crea y es quien lo itera (ver #2). **Ningún otro artefacto**; el handoff a `/opsx:propose` sigue siendo explícito. |
| 2 | `context.md` refresh si cambia el grafo | Manual vía `/opsx:explore <change-name>`. Nunca auto-refresh. No hay comando de refresh aparte. **Enmendada por la #13**: explore es el **dueño** de `context.md` (lo crea, lo itera y es el único que regenera la mitad derivada); `/opsx:propose` es un **colaborador acotado**, solo sobre su propia mitad conversacional — **puede** crearlo si no existe, appendear a "Entrevista de intención" las Q&A que **él** condujo (verificación de completitud y ajustes del gate) y actualizar el intent brief con respuestas que él relevó; **nunca** reescribe/reordena/borra Q&A de explore, nunca re-sintetiza el brief desde la conversación y nunca regenera la mitad derivada (deja el marcador `_Pendiente — correr /opsx:explore <change-id>_` y lo avisa). La tabla del corte vive en `propose.md` paso 1. |
| 3 | `ui-ux.md` siempre o solo si aplica | Siempre. Veredicto obligatorio en la primera línea. |
| 4 | Falla a mitad de `apply` | Borrar `openspec/changes/<name>/` a mano. `archive` no se usa para deshacer. |
| 5 | Trigger de `documenter distill` | Automático en `archive` (con id dateado). Manual para re-runs. La instrucción vive en `config.yaml` → `operations.archive.guidance` (canal de OpenSpec >= 1.9.0), no hardcodeada en el markdown; `archive` la lee vía `openspec instructions archive --json`. El paso 7 de `archive.md` queda como fallback si el lookup falla: acá el distill **no es advisory**. |
| 6 | Validación pre-archive | Gate **bloqueante** en `archive.md` paso 4: `openspec validate <name> --type change --strict`, más 3 chequeos advisory (glob de `knowledge/modules/*/use-cases/<slug>/`, última task = distill, marca de aprobación del proposal). En CI: `openspec validate --changes --strict`. Desde 1.9.0 el `--strict` pelado no valida nada y sale con exit 1: siempre pasar un scope (`--changes` / `--specs` / `--all`) o un item. |
| 7 | Multi-capability changes | Soportado: múltiples carpetas en `specs/`. |
| 8 | Namespace de comandos | `/opsx:` — **4 comandos de flujo** (explore, propose, apply, archive) + **1 subrutina** (`sync`, que `archive` corre inline; no se invoca a mano en el flujo normal). Set fijo: no se agregan comandos sin revisitar el diseño. `sync` entró en el upgrade a OpenSpec 1.9.0 porque el `archive` de vanilla delega el merge de delta specs en él. `refresh-context` **existió y se dio de baja**: desde que `context.md` tiene una sección conversacional, dos escritores del mismo archivo duplican el cuidado de no destruirla — la iteración quedó en su autor (`/opsx:explore <name>`). Vanilla 1.9.0 trae además `new`, `continue`, `update`, `ff`, `bulk-archive`, `verify` y `onboard`: **deliberadamente no se instalan** (`propose` cubre `new` + `continue`). |
| 9 | `flow.md` por use-case | Obligatorio en cada use-case, veredicto en la 1ª línea (`Aplica` / `No aplica — flujo trivial`). Modo *explanation*: flujo funcional en prosa numerada (canónica) + diagrama Mermaid (vista derivada). Refs por ancla estable (`graph://node:X` / símbolo), nunca por número de línea. Es destino de `documenter distill`, **NO** un 7º artefacto del change. Lleva además `Permisos` y `Casos de error / rechazo` (vista tabular derivada de scenarios). |
| 10 | Módulos contienen use-cases | El árbol de `knowledge/` es anidado: `modules/<module>/use-cases/<capability>/`. El módulo es navegación/dominio (con `INDEX.md`: resumen + diagrama + tabla intent→flow + notas transversales); el use-case es contrato/cambio (1:1 con `openspec/specs/`). **Ubicación = membresía** (sin frontmatter `module:`); `distill` resuelve por **glob** del slug único. `_unsorted` reservado para nuevos sin clasificar. Los módulos NO sincronizan por nombre. Sin 4º subcomando: se extienden `bootstrap` (crea `modules/` + INDEX global; siembra desde comunidades Graphify) y `distill`. Citación por ancla estable. |
| 11 | Comando `/debug` | Vive **fuera** del namespace `/opsx:` y **no** es un 6º comando del flujo: no genera artefactos ni escribe en `openspec/`. Tres fases: *Understand* (preguntas hasta cerrar hipótesis explícita) → *Investigate* (código guiado por la hipótesis, root cause con `file:line`) → *Verify* (checklist de evidencia; lo no verificado se declara). Output fijo = **bug report**, que es input de `/opsx:propose fix-<algo>` o descripción del PR/issue. **Termina ahí**: aplicar el fix requiere autorización explícita del humano; describir la dirección del fix no la implica. |
| 12 | Versión de OpenSpec y drift de los forks | **Piso: OpenSpec >= 1.9.0.** Los `/opsx:*` son forks de los templates `opsx` de esa versión y dependen de contratos que antes no existían (`resolvedOutputPath`, `artifactPaths`, `planningHome`, `instructions apply\|archive --json`, status `skipped` vía `skip_specs`). Toda desviación respecto de vanilla se marca con `⚠️ LOCAL CUSTOMIZATION` en el markdown y se lista abajo, para poder re-aplicarla en el próximo upgrade. Ver [README.md § Contratos de CLI](README.md#contratos-de-cli-que-usa-el-flujo). |
| 13 | Gate de aprobación del proposal | `/opsx:propose` **no entrevista**: el paso 1 es una **verificación de completitud** del input (intent brief de explore / bug report de `/debug` / pedido directo) contra el checklist intención · alcance · comportamiento observable · non-goals · criterios de aceptación, con routing: completo → cero preguntas; huecos puntuales → una pregunta por hueco; hueco grande → sugerir `/opsx:explore` (nombrando el change si existe); change existente con `context.md` → se respeta y se arranca en `proposal`. Después de escribir `proposal.md`, el loop tiene **una única pausa deliberada**: el **proposal gate**, disparado **por estado en disco** (existe el artefacto con id `proposal` — de `status --json`, nunca hardcodeado — y su marca `**Estado:**` no dice `Aprobado`), nunca por evento — así un re-run re-gatea solo lo no aprobado. Mientras no se apruebe, no se piden `instructions` de nada que dependa de `proposal` directa o transitivamente. Interfaz: resumen releído de disco + AskUserQuestion `Approve`/`Adjust`/`Cancel` **sin opción recomendada**, con fallback en prosa; respuesta ambigua → repreguntar (describir ajustes no es aprobar). La marca la escribe **solo propose** y solo tras un Approve explícito en el mismo turno (`Aprobado — <fecha>`); **toda (re)generación resetea a `Pendiente de aprobación`** (se aprueba un texto, no un archivo). Schema sin artefacto `proposal` → gate omitido con aviso. `archive` chequea la marca como advisory (warning, nunca bloquea — changes pre-marca archivan igual). |

Meta-arquitectura:
- La lógica del proyecto vive en `openspec/config.yaml` (`context:` + `rules:` + `operations:`) + los
  templates del schema. **No** se duplica en el markdown de los comandos `/opsx:*`.
- Los comandos `/opsx:*` son forks mínimos de los templates `opsx` de vanilla.
- `context`, `rules` y `operationGuidance` son **constraints para el agente, nunca contenido del artefacto**.

### Registro de customizaciones sobre vanilla 1.9.0

Todo lo que sigue es drift deliberado: hay que re-aplicarlo cuando se actualice OpenSpec. Lo demás en
`.claude/commands/opsx/` es vanilla textual y se puede reemplazar sin pensar.

| Archivo | Desviación | Por qué |
|---|---|---|
| `propose.md` | Bloque de cabecera con los 6 artefactos de `sdd-with-knowledge` + mapeo capability ↔ use-case + regla de capability slug **plano** | Vanilla describe el schema `spec-driven` de 4 artefactos y sugiere capability paths anidados; acá el slug es plano (la regla 1:1 por glob depende de eso) |
| `propose.md` | Paso 1 reescrito como **Completeness check** (checklist intención/alcance/comportamiento/non-goals/criterios + routing completo/huecos/hueco-grande/change-existente + tabla del corte de escritura sobre `context.md`) | Vanilla clarifica una sola vez y solo ante "material ambiguity"; acá propose es el único paso obligatorio del ciclo y la red de seguridad de intención (decisiones #2 enmendada y #13) |
| `propose.md` | Sub-paso **Proposal gate** en el paso 5 (pausa única por estado en disco, resumen releído, AskUserQuestion `Approve`/`Adjust`/`Cancel` + fallback en prosa, marca `**Estado:**`) + línea de gate en el Output + 2 guardrails (marca solo tras Approve; corte sobre `context.md`) + aviso de la pausa en la cabecera | El loop de vanilla no frena nunca ("dependencies are enablers, not gates"); el estado vive en `proposal.md` porque `openspec status` es solo existencia de archivo (decisión #13) |
| `apply.md` | Banner del set reducido; el estado `blocked` manda a `/opsx:propose` | `/opsx:continue` no está instalado |
| `archive.md` | Paso 4 (gate de validación **bloqueante** + 3 chequeos advisory: glob del use-case, última task = distill, marca de aprobación del proposal) + paso 7 (distill) + línea `Knowledge:` en los outputs + 2 guardrails | El fork manual del archive perdió el validate-por-default del `openspec archive` de vanilla; el distill es obligatorio acá (fallback de `operations.archive.guidance`); la marca es advisory porque los changes pre-gate no la tienen (decisión #13) |
| `explore.md` | 4 bullets de profundidad (*inquisitive* / *deep* / *why* / *beyond the entry point*) + bloque Graphify/knowledge + sección **Convergent Close** (entrevista → intent brief → escritura **siempre** de `context.md` → oferta recomendada del snapshot técnico, con marcador de pendiente si se declina) + sección **Iterating an Existing `context.md`** (append de la entrevista, regeneración de la mitad derivada, diff + confirmación, heads-up downstream) + guardrails "Don't skip the interview" / "Don't leave the interview unsaved" / "Don't rewrite the interview" / "`context.md` is yours; the other five aren't" + matiz en "Don't auto-capture" + bullets de cierre e iteración en *Ending Discovery* | Vanilla 1.9.0 recortó los bullets de profundidad; el bloque aterriza el `context:` en herramientas concretas; el cierre convergente no existe en vanilla (explore es 100% divergente y sin contrato de salida) y es lo que hace que `/opsx:propose` arranque de lo que el usuario respondió, no de la síntesis del agente. La sección de iteración absorbió al comando `refresh-context` (dado de baja): resolución de paths por CLI, diff + confirmación y heads-up downstream venían de ahí |
| ~~`refresh-context.md`~~ | **Dado de baja** — su contenido vive ahora en § *Iterating an Existing `context.md`* de `explore.md` | Un solo dueño de `context.md`: desde schema v4 el archivo tiene una sección conversacional que no se puede regenerar sin destruir el registro de la entrevista, y el que puede iterarla con criterio es su autor (explore). **Nota de upgrade:** `install-snappec.sh --force` copia pero no borra — en un target ya instalado el `refresh-context.md` viejo sobrevive y hay que eliminarlo a mano (`rm .claude/commands/opsx/refresh-context.md`) |
| `sync.md` | — (vanilla textual) | Se instala sólo porque `archive` lo necesita inline |
