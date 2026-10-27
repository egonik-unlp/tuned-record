# Tasks: {{CHANGE_ID}}

<!-- Tasks de implementación en orden. Ver design.md para el approach técnico. -->

## Implementation

- [ ]
- [ ]
- [ ]

## Validation

- [ ] `npx tsc --noEmit` sin errores
- [ ] `yarn lint` sin errores nuevos
- [ ] Test manual del flujo principal

## Post-archive

- [ ] `/documenter distill <dated-change-id>` — **auto-ejecutado por `/opsx:archive`**.
      Solo correr a mano si el auto-trigger falló (re-run con el id completo con fecha,
      ej. `2026-05-26-add-foo`). Sincroniza el use-case (knowledge/modules/<m>/use-cases/) con los deltas del change
      (spec narrativa, decisiones, lore). Humano revisa y acepta edits propuestos.
