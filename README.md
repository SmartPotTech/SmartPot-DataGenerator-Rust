# SmartPot DataGenerator (Rust)

[![Rust CI](https://github.com/SmartPotTech/SmartPot-DataGenerator-Rust/actions/workflows/ci.yml/badge.svg)](https://github.com/SmartPotTech/SmartPot-DataGenerator-Rust/actions/workflows/ci.yml)
[![Publish Docker Images](https://github.com/SmartPotTech/SmartPot-DataGenerator-Rust/actions/workflows/packaging.yml/badge.svg)](https://github.com/SmartPotTech/SmartPot-DataGenerator-Rust/actions/workflows/packaging.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-00B074.svg)](LICENSE)

Simulador de cultivos y dispositivos hidropónicos virtuales para la plataforma **SmartPot**, reescrito en **Rust** para máximo rendimiento, seguridad de memoria sin recolector de basura (*garbage collection*) y concurrencia asíncrona de alta eficiencia.

Habla el contrato **MQTT v1** de SmartPot exactamente igual que un dispositivo físico ESP32, y expone una API REST interna protegida con tiempo constante (*constant-time comparison*) para administración dinámica de cultivos en tiempo de ejecución.

---

## Características principales

- **Rendimiento y eficiencia extrema**: Implementado sobre el runtime asíncrono **Tokio** y el framework web **Axum 0.8**, con una huella de memoria inferior a 15 MB RAM y tiempos de respuesta sub-milisegundo.
- **Seguridad de memoria estricta**: `#![deny(unsafe_code)]` en todo el proyecto. Sin punteros crudos ni bloques `unsafe`.
- **Protección contra ataques de canal lateral**: Validación del token de servicio (`SIMULATOR_TOKEN`) mediante comparación en tiempo constante (`subtle::ConstantTimeEq`) para prevenir *timing attacks*.
- **Contrato MQTT v1 estricto**:
  - `smartpot/v1/{cropId}/telemetry`: Publicación periódica QoS 0 con lecturas físicas y ruido gaussiano realista.
  - `smartpot/v1/{cropId}/commands`: Recepción y ejecución de órdenes a actuadores (`WATER_PUMP`, `UV_LIGHT`, `FAN`, `HUMIDIFIER`, `NUTRIENT_DOSER`, `PH_DOSER`).
  - `smartpot/v1/{cropId}/commands/ack`: Confirmación inmediata QoS 1 con mensajes legibles en español.
  - `smartpot/v1/{cropId}/status`: Estado retenido (*retained*) QoS 1 con LWT (*Last Will and Testament*) `online`/`offline`.
- **Simulación física continua**:
  - Modos: `AUTO` (ciclo circadiano día/noche según zona horaria), `MANUAL` (medidores configurables) y `WEATHER` (integración con el clima real).
  - Filtrado ambiental por ubicación (`INDOOR` / `OUTDOOR`) y exposición solar (`FULL_SUN`, `PARTIAL_SUN`, `SHADE`).
  - Dinámica de actuadores: enfriamiento, humidificación, radiación de luz, riego, evaporación y dosificación química.
- **Integración con Open-Meteo**: Caché en memoria con TTL de 10 minutos y resolución por cuadrícula geográfica para geocodificación y pronósticos meteorológicos sin claves ni cuotas.
- **Contenedor seguro y mínimo**: Imagen multi-etapa basada en `debian:bookworm-slim` sin privilegios (`uid: 1000`, `cap_drop: ALL`, `read_only: true`) y healthcheck nativo integrado (`--healthcheck`).

---

## Variables de entorno

| Variable | Tipo | Por defecto | Descripción |
| --- | --- | --- | --- |
| `MQTT_HOST` | String | `localhost` | Host del broker MQTT (ej. `broker-smartpot` o `mqtt.smartpot.app`). |
| `MQTT_PORT` | Entero | `1883` (o `8883` con TLS) | Puerto de conexión MQTT. |
| `MQTT_TLS` | Booleano | `false` | Habilita TLS para la conexión al broker. |
| `MQTT_CA_FILE` | String | Vacío | Ruta al certificado CA en formato PEM (opcional). |
| `MQTT_TOPIC_PREFIX` | String | `smartpot/v1` | Prefijo de tópicos MQTT. |
| `SIMULATOR_DEVICES` | String | Vacío | Dispositivos estáticos protegidos en formato `cropId:clave:TIPO` separados por comas. |
| `SIMULATOR_INTERVAL_SECONDS` | Float | `30` | Intervalo base de publicación de lecturas de telemetría. |
| `SIMULATOR_UTC_OFFSET` | Float | `-5` | Diferencia horaria respecto a UTC para el ciclo solar. |
| `SIMULATOR_TOKEN` | String | Vacío | Token Bearer para la API de control (mínimo 24 caracteres). |
| `SIMULATOR_PORT` | Entero | `8081` | Puerto de escucha de la API HTTP interna. |
| `RUST_LOG` | String | `info` | Nivel de logs estructurados (`error`, `warn`, `info`, `debug`, `trace`). |

---

## API REST de Control

La API escucha en el puerto `8081` (configurable vía `SIMULATOR_PORT`).

### Endpoints

- `GET /health`: Público. Chequeo de salud del contenedor y estado de cultivos conectados.
- `GET /v1/pots`: Requiere `Authorization: Bearer <TOKEN>`. Lista el estado completo de todos los cultivos simulados.
- `GET /v1/pots/{crop_id}`: Requiere Bearer. Retorna el snapshot del cultivo indicado (o 404).
- `PUT /v1/pots/{crop_id}`: Requiere Bearer. Crea o actualiza un cultivo virtual dinámico. Retorna 409 si el cultivo es estático de `SIMULATOR_DEVICES`.
- `DELETE /v1/pots/{crop_id}`: Requiere Bearer. Detiene y retira un cultivo virtual dinámico (204 No Content).
- `GET /v1/places?q={nombre}`: Requiere Bearer. Búsqueda de lugares geográficos mediante geocodificación.
- `GET /v1/weather?latitude={lat}&longitude={lon}`: Requiere Bearer. Consulta el clima actual para las coordenadas dadas.

---

## Ejecución local y pruebas

### Requisitos

- Rust 1.80+ (o superior)
- Cargo

### Compilación y pruebas

```bash
# Verificar tipos y compilación
cargo check

# Ejecutar suite de pruebas unitarias e integración (23 pruebas)
cargo test

# Ejecutar el linter estricto
cargo clippy --all-targets -- -D warnings

# Ejecutar localmente
cargo run
```

### Construcción con Docker

```bash
docker build -t smartpot-datagenerator:latest .
docker run --rm -p 8081:8081 smartpot-datagenerator:latest
```
