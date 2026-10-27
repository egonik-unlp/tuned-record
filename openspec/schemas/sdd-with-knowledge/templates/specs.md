# Delta Spec: {{CHANGE_ID}}

<!-- Delta spec de la capability afectada. Si el change toca más de una capability,
     crear un archivo por capability en specs/<capability>/spec.md (el slug coincide 1:1
     con el use-case en knowledge/modules/<m>/use-cases/<capability>/ y es SIEMPRE un
     slug plano de un solo segmento: user-auth, nunca identity/user-auth).

     REGLAS DE FORMATO (las valida `openspec validate <change> --strict`):
     - Cada requirement:  ### Requirement: <Nombre>   (literal "Requirement:"; el nombre
       es la CLAVE DE MERGE contra la spec viva — único y estable)
     - El texto del requirement DEBE contener SHALL o MUST (en inglés, mayúsculas).
     - Cada requirement necesita >= 1 scenario con header de EXACTAMENTE 4 numerales:
       #### Scenario: <nombre>   — bullets sin header o 3 numerales fallan silenciosamente.
     - Borrar las secciones que no se usen. El delta contiene SOLO lo que cambia. -->

**Capability:** {{CAPABILITY_NAME}}
**Change:** {{CHANGE_ID}}

<!-- SOLO si la capability es NUEVA, agregar antes de ADDED una sección Purpose
     (archive la copia a la spec viva). Si la capability ya existe, NO incluirla —
     se ignora con warning. Formato:

       ## Purpose
       <una o dos oraciones sobre para qué existe esta capability>
-->

## ADDED Requirements

<!-- Requirements nuevos, completos, con sus scenarios. Formato (copiar SIN la indentación):

       ### Requirement: <Nombre estable del requirement>
       El sistema SHALL <comportamiento observable, no implementación>.

       #### Scenario: <nombre descriptivo>
       - **GIVEN** <estado inicial>
       - **WHEN** <acción>
       - **THEN** <resultado esperado>
-->

## MODIFIED Requirements

<!-- Requirements existentes que cambian. El header debe matchear EXACTO el nombre
     en openspec/specs/{{CAPABILITY_NAME}}/spec.md — el merge es por nombre.
     Va el requirement ENTERO y actualizado (texto + TODOS los scenarios que sobreviven,
     no solo lo que cambió): archive reemplaza el bloque completo, y un MODIFIED que
     pierde scenarios de la spec viva no pasa la validación. -->

## REMOVED Requirements

<!-- Solo el header con el nombre exacto + razón y migración:

       ### Requirement: <Nombre exacto en la spec viva>
       **Reason**: <por qué se elimina>
       **Migration**: <qué usar en su lugar>
-->

<!-- Si solo cambia el NOMBRE de un requirement, usar RENAMED en vez de REMOVED+ADDED:

       ## RENAMED Requirements
       - FROM: `### Requirement: <Nombre viejo>`
       - TO: `### Requirement: <Nombre nuevo>`
-->
