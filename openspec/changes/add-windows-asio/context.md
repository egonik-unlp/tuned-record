# Context: add-windows-asio

## Entrevista de intención

### Q&A

| Pregunta | Respuesta |
|---|---|
| ¿Qué backend de audio en Windows? | cpal con el host ASIO. Linux queda como está (JACK sobre PipeWire). |
| ¿Se permite refactorizar el engine para compartir el núcleo con Windows? | Sí (opción A): extraer un core compartido; el comportamiento en Linux no cambia, comprobado con `loopback_check`. |
| ¿Para quién es? | Uso personal; la va a usar alguien menos técnico que el dueño. |
| ¿Dónde y cómo se compila? | Idealmente acá (en esta máquina Linux, cross-compile), sin CI. |
| ¿Qué expectativa de robustez? | Que se pueda usar, idealmente sin reinicios; puede tener instalador. |
| ¿Qué hardware hay del lado de Windows? | Interfaz de audio con driver ASIO propio. |
| ¿Para qué se usa en Windows? | Practicar y grabar tomas (la alineación de las tomas importa). |
| ¿Quién valida en Windows? | El dueño, en su propia máquina Windows. |
| ¿Qué tiene que ser cierto para darlo por terminado? | En la máquina Windows del dueño con la interfaz ASIO: `loopback_check --physical` y Calibrate… dentro de 1 ms, una toma grabada alinea, y un reset del driver se recupera sin reiniciar la app. En Linux, `loopback_check` da lo mismo que hoy. |
| ¿Qué queda explícitamente fuera? | Firma de código, fallback WASAPI/FlexASIO, macOS, auto-update. |
| ¿Quién calibra la latencia en la PC de la otra persona? | El dueño, una vez, con un cable; la otra persona solo practica y graba. |
| ¿Cuándo entra respecto a los changes abiertos? | Después de archivar ambos: `add-pitch-follow-recorder` y luego `add-practice-aids`. |

### Intent brief

**Intención:** Make tuned-record run natively on Windows so a less technical person can practise and record takes on their own PC. Personal use; the owner builds it here and sets it up for them. The Linux/JACK version stays exactly as it is.

**Alcance:** A Windows audio backend on cpal's ASIO host. The engine cycle (commands, song clock and speed, mixing, block stamping, mic streams) moves into a shared core, with JACK (Linux) and ASIO (Windows) drivers on top. The Windows driver recovers from ASIO resets (buffer size or rate change, unplug) without restarting the app. The device dialog lists the ASIO driver's inputs and outputs. Platform-only dependencies and code are gated per target. A cross-compile from this Linux box to x86_64-pc-windows-gnu, an NSIS installer, and a Wine smoke test. The audio-engine spec and project.md are worded per platform.

**Comportamiento observable:** On Windows the app installs from a setup .exe, opens without a console window, and works with the interface's ASIO driver. The mic input and output are picked in the same device dialog. Playback, the live pitch line, note bars, speeds, the level meter, recording and Calibrate… behave as on Linux. A driver reset (buffer size change in the driver panel, unplugging) recovers on its own: transport position, playback, loop and speed are kept, and a take in progress is closed cleanly. On Linux nothing changes.

**Non-goals:** Code signing (SmartScreen is clicked through once, following a one-line note). WASAPI/FlexASIO or interfaces without a native ASIO driver. macOS. Auto-update. CI. Resuming a take that a driver reset interrupted.

**Criterios de aceptación:** On the owner's Windows machine with the ASIO interface, loopback_check --physical and Calibrate… land within 1 ms, a recorded take lines up with the backing, and a driver reset recovers without restarting the app. On Linux, loopback_check gives the same results as before: 0.000 ms at 100 %, under 2 ms at 60 %. The Windows build and installer are produced on this Linux box. Calibration on the other person's PC is done once by the owner with a cable.

**Incertidumbres abiertas:** Whether the whole crate (asio-sys plus the signalsmith C++ stretcher via bindgen) cross-compiles and links for windows-gnu here; that is spike #1. Whether the user's ASIO driver keeps cpal's input-before-output callback order; a misorder would be a constant one-period offset that calibration absorbs. How a take is closed and marked when a reset interrupts it. Where projects and config live on Windows. The Steinberg ASIO SDK licence terms, not verified. Sequencing: starts only after add-pitch-follow-recorder and add-practice-aids are archived, so its spec deltas apply to clean main specs.

## Nodos afectados

> **Limitación:** Graphify MCP no conectó en esta sesión y el repo no tiene `graphify-out/` ni
> `knowledge/`. Este snapshot sale de lectura directa del código y de los specs de los changes abiertos,
> no del grafo; no hay referencias `graph://`. Regenerar con `/opsx:explore add-windows-asio` cuando
> Graphify esté disponible.

- **`audio::engine`**: núcleo del cambio. Hoy mezcla dos cosas: el ciclo de audio independiente de
  plataforma (`Command`, `Shared`, `BlockHeader`/`BlockKind`, `MicStream`/`StreamProducer`, el
  `Processor` con `SongClock`, mezcla y estampado) y el driver JACK (`impl jack::ProcessHandler`,
  `impl jack::NotificationHandler`, `Engine` con `Client`, `preload_pipewire_jack`,
  `request_low_latency`, ruteo por nombre de puerto). Se parte en core + `jack` + `asio`.
- **API pública de `Engine` que la UI consume** (tiene que existir igual en Windows): `start`,
  `start_routed`, `shared`, `send`, `collect_garbage`, `refresh_latency`, `refresh_routing`,
  `keep_routed`, `input_sources`, `output_devices`, `set_input`, `set_output`, `routing`,
  `buffer_size`, `speed`, `latency_ms`, `device_of`, `short_port`. Solo JACK: `client`, `port_names`.
- **Consumidores de la API**: `ui::app` (arranque con `start_routed`, `set_input`/`set_output`,
  `server_gone`), `ui::panels` (diálogo de dispositivos, botón de dispositivos), `main` 
  (`request_low_latency`), ejemplos `loopback_check` y `probe`.
- **Sin cambios esperados, pero tienen que compilar para Windows**: `audio::{mixer, speed, stretch,
  latency}`, `pitch::*`, `feedback`, `project::*`, `ui::{plot, session, theme, timeline}`, `config`.

## Archivos relevantes

| Archivo | Rol |
|---|---|
| `src/audio/engine.rs` | Engine actual (core + JACK mezclados); se divide en `audio/engine/{mod,core,jack,asio}.rs` |
| `src/audio/latency.rs` | Calibración/compensación; la fuente de latencia pasa a depender del driver (puertos JACK vs `ASIOGetLatencies` vía timestamps de cpal) |
| `src/audio/mixer.rs`, `src/audio/speed.rs` | Usados por el core desde los dos drivers; sin cambios |
| `src/audio/stretch.rs` | Envuelve `signalsmith-stretch` (C++ vía bindgen): riesgo de cross-compile |
| `src/main.rs` | `request_low_latency` pasa a ser solo Linux; agregar `windows_subsystem = "windows"` |
| `src/ui/app.rs` | Arranque del engine, cambio de dispositivo, aviso de servidor caído (que en Windows pasa a ser recuperación de reset) |
| `src/ui/panels.rs` | Diálogo de dispositivos sobre strings `"device:port"` (`device_of`/`short_port`); en Windows `"<driver ASIO>:in N"` |
| `src/config.rs` | `dirs::config_dir()` → `%APPDATA%\tuned-record\config.toml` en Windows; guarda `input_port`/`output_device` |
| `examples/loopback_check.rs` | Validación de alineación; usa `engine.client()`/`port_names()`/`jack::PortFlags` para el auto-patch, que en Windows es solo `--physical` |
| `examples/probe.rs` | Diagnóstico del engine; usa `request_low_latency` |
| `Cargo.toml` | `jack` y `libloading` pasan a `[target.'cfg(target_os = "linux")'.dependencies]`; `cpal` con feature `asio` en `cfg(windows)` |
| `openspec/project.md` | "Linux desktop tool", "Linux only; needs `pipewire-jack`", regla RT de "no locks" sobre el callback JACK |
| `openspec/changes/add-pitch-follow-recorder/specs/audio-engine/spec.md` | Requirements "Duplex audio over JACK on PipeWire", "Audio device selection", "Low-latency buffer request", "Latency-compensated capture", "Real-time safe processing": hay que calificarlos por plataforma |
| `openspec/changes/add-practice-aids/specs/audio-engine/spec.md` | Deltas de "Single song clock" y "Latency-compensated capture" (velocidad); el core los tiene que conservar en los dos drivers |
| (nuevo) script de build + `installer/*.nsi` | Cross-compile a `x86_64-pc-windows-gnu` y empaquetado NSIS |

## Contexto knowledge

No existe `knowledge/` en el repo todavía (ni `decisions.md`, ni módulos, ni use-cases), así que no
hay decisiones macro ni use-case resuelto por glob para contrastar. Lo que hace de "contexto
curado" hoy:

- **`openspec/project.md`**: el producto se define como herramienta de escritorio solo para Linux,
  con audio por el crate `jack` sobre `pipewire-jack` y precarga de libjack de PipeWire. La regla de
  tiempo real dice que el callback no hace allocations, locks ni I/O. Este change contradice la
  primera parte (plataforma) a propósito, y tiene que matizar la segunda: `asio-sys` toma un mutex
  sin contención por período en su propio dispatch, fuera del código de la app.
- **Capability `audio-engine`** (en los specs delta de los dos changes abiertos, sin main spec todavía
  porque no hay nada archivado): un solo reloj de canción, duplex, captura compensada por latencia
  con un escenario de 1 ms en loopback, selección de dispositivos, procesamiento RT-safe y, desde
  `add-practice-aids`, velocidad de práctica con el reloj a `p/q`.
- **Hallazgos del explore** (registrados acá porque no hay `knowledge/`):
  - cpal 0.18.2 ASIO despacha los callbacks de input y output en orden de registro, en el mismo
    hilo, con el mismo `buffer_index` y `system_time`, así que se comporta como duplex. Construir
    primero el stream de input y etiquetar el bloque del mic con `t_k`.
  - Un solo driver ASIO por proceso: input y output en la misma interfaz.
  - Formato de muestra del driver (a menudo i32) exacto; convertir en buffers preasignados.
  - El tamaño de buffer se fija en el panel del driver, no desde la app.
  - `asio-sys` 0.4.0 tiene un build.rs que distingue el target, soporta MinGW, toma el SDK de
    `CPAL_ASIO_DIR` o lo descarga, y necesita libclang (igual que `signalsmith-stretch`).
  - En esta máquina falta `mingw-w64`, el target de Rust `x86_64-pc-windows-gnu` y `makensis`.
    Wine está instalado.

## Riesgos identificados

- **Contradicción con `project.md`**: "Linux only" y la regla RT sin locks. Se resuelve a propósito
  en este change (redacción por plataforma, salvedad explícita para el mutex de `asio-sys`).
- **Specs sin archivar**: la capability `audio-engine` vive solo en los deltas de
  `add-pitch-follow-recorder` y `add-practice-aids`. Este change tiene que esperar a que se archiven
  los dos (en ese orden) para que sus deltas apliquen sobre main specs limpios.
- **Regresión en Linux** al extraer el core: es el consumidor más crítico. Mitigación: comparar
  `loopback_check` antes y después (0.000 ms a 100 %, < 2 ms a 60 %) y que pasen los 70 tests.
- **Cross-compile no probado**: `asio-sys` + `signalsmith-stretch` (C++/bindgen) + eframe para
  `windows-gnu`, con libstdc++ estática. Spike #1, antes de tocar el engine.
- **Orden de callbacks del driver**: si un driver ASIO no respeta input-antes-que-output, aparece un
  offset fijo de un período. Lo absorbe la calibración, pero hay que medirlo en el hardware real.
- **Recuperación de reset del driver**: reconstruir los streams fuera del hilo RT, conservar
  posición/playback/loop/velocidad, y cerrar limpia una toma interrumpida, que no se reanuda.
  Reemplaza el camino actual de `server_gone`, que hoy es fatal.
- **Diálogo de dispositivos**: elegir un mic de otro driver ASIO cambia también la salida (un driver
  por proceso). Es comportamiento nuevo que la UI tiene que hacer visible.
- **Ejemplos acoplados a JACK**: `loopback_check` (auto-patch con `jack::PortFlags`) y `probe` no
  compilan en Windows sin `cfg`.
- **Usuario no técnico**: SmartScreen sin firma; la calibración la hace el dueño una vez.
- **Licencia del SDK ASIO de Steinberg**: no verificada; uso personal.
