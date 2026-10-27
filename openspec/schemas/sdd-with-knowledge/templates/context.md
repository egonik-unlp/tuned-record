# Context: {{CHANGE_ID}}

<!-- Consolidar contexto ANTES de escribir la proposal.
     Este artefacto reduce tokens de exploración downstream.

     El archivo tiene DOS MITADES con reglas distintas. El dueño de las dos es /opsx:explore: lo crea
     en su cierre convergente y es también quien lo itera (no hay comando de refresh aparte).
     /opsx:propose es colaborador acotado SOLO de la mitad conversacional: puede crear el archivo si no
     existe (camino directo) y appendear a la entrevista las Q&A que él mismo condujo — nunca toca la
     mitad derivada (deja los marcadores _Pendiente_).
     1. "Entrevista de intención" — conversacional. La escribe quien condujo la entrevista y crece por
        APPEND: las Q&A nuevas van al final; las ya registradas no se reescriben, reordenan ni borran.
        El intent brief se actualiza con respuestas reales, nunca se re-sintetiza de la conversación.
     2. "Nodos afectados" / "Archivos relevantes" / "Contexto knowledge" / "Riesgos identificados" —
        snapshot derivado de Graphify + knowledge/. Es regenerable: se reescribe completo al iterar.
        Si todavía no se consolidó, cada una de esas 4 secciones lleva el marcador
        `_Pendiente — correr /opsx:explore <change-id> para consolidar el snapshot técnico._`
        en lugar de quedar vacía (una sección vacía se lee como "no hay nodos / no hay riesgos"). -->

## Entrevista de intención

<!-- Registro de la entrevista de intención (la completa quien la condujo: el cierre convergente de
     /opsx:explore — que SIEMPRE la persiste acá y la extiende al iterar — o /opsx:propose).
     - Q&A relevantes EN LIMPIO: una línea por par pregunta → respuesta. No transcript literal:
       se descartan los idas y vueltas irrelevantes y se normaliza la redacción sin cambiar el sentido.
     - Después, el intent brief resultante con sus 6 secciones (Intención / Alcance /
       Comportamiento observable / Non-goals / Criterios de aceptación / Incertidumbres abiertas).
     Esta sección es autoría humana/conversacional: NO se regenera. Al iterar se le agregan Q&A al
     final. Si no hubo entrevista, dejarla explícitamente vacía ("Sin entrevista"), no inventarla. -->

### Q&A

| Pregunta | Respuesta |
|---|---|
| | |

### Intent brief

**Intención:**
**Alcance:**
**Comportamiento observable:**
**Non-goals:**
**Criterios de aceptación:**
**Incertidumbres abiertas:**

## Nodos afectados

<!-- Derivado de Graphify MCP (query_graph / get_neighbors / shortest_path).
     Listar nodos/comunidades relevantes con referencias graph://node:X o graph://community:X. -->

## Archivos relevantes

<!-- Paths derivados del subgrafo, NO de grep manual.
     Una fila por archivo; indicar rol (entry point, service, util, etc.) si ayuda. -->

| Archivo | Rol |
|---|---|
| | |

## Contexto knowledge

<!-- Resumen de knowledge/ relacionado (use-case resuelto por glob knowledge/modules/*/use-cases/<name>/):
     - Decisiones macro que aplican (knowledge/decisions.md)
     - Navegación del módulo (knowledge/modules/<m>/INDEX.md)
     - Spec actual del use-case si existe (knowledge/modules/<m>/use-cases/<name>/spec.md)
     - Flujo funcional del use-case (knowledge/modules/<m>/use-cases/<name>/flow.md)
     - Decisiones del use-case (knowledge/modules/<m>/use-cases/<name>/decisions.md)
     - Notas/lore del use-case (knowledge/modules/<m>/use-cases/<name>/notes.md)
     - Convenciones aplicables (knowledge/conventions.md) -->

## Riesgos identificados

<!-- Conflictos potenciales detectados durante la consolidación:
     - Contradicciones con decisions.md
     - Cambios que afectan consumidores (derivar de Graphify, no de docs)
     - Deuda técnica conocida en el subgrafo afectado
     "Ninguno" si no hay riesgos. -->
