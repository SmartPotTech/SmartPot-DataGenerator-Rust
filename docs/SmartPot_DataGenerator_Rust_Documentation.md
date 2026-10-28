<!-- portada
eyebrow: Documentación del componente
titulo: SmartPot-DataGenerator-Rust
acento: DataGenerator-Rust
subtitulo: El simulador de alto rendimiento de SmartPot en Rust
bajada: Cultivos virtuales de alto rendimiento y bajo consumo que hablan el contrato MQTT v1: modelo físico, Tokio runtime, Axum 0.8, rumqttc, seguridad de memoria estricta y API de control interna.
documento: SmartPot-DataGenerator-Rust
version: 2.0 · octubre 2026
equipo: SmartPotTech
proyecto: smartpot.app
-->

# SmartPot-DataGenerator-Rust

## Ficha del documento

| Campo | Valor |
| --- | --- |
| Proyecto | SmartPot · [smartpot.app](https://smartpot.app) |
| Componente | [SmartPot-DataGenerator-Rust](https://github.com/SmartPotTech/SmartPot-DataGenerator-Rust) |
| Versión | 2.0.0 · octubre 2026 |
| Lenguaje y runtime | Rust 1.85+ · Tokio · Axum 0.8 · rumqttc |
| Alcance | Cultivos virtuales, cultivos fijos para demo y QA, modelo físico agronómico, API de control REST interna, seguridad de memoria, pruebas y despliegue |
| Estado | Migrado y validado en `demo-beta` |

---

## 1. Propósito y motivación de la migración

### En palabras simples

El simulador da vida a los **cultivos virtuales**: los que un usuario crea en SmartPot sin hardware. Corre permanentemente como servicio de soporte en la plataforma y realiza el mismo trabajo que un microcontrolador ESP32 físico:
1. Publica telemetría periódica por MQTT con las credenciales del cultivo.
2. Escucha comandos hacia actuadores y los ejecuta en el modelo físico.
3. Responde inmediatamente con confirmaciones legibles (*ACK*) por MQTT.
4. Mantiene el estado retenido de conexión (`online` / `offline`) mediante LWT.

La versión original en Python 3.13 cumplía la funcionalidad básica pero presentaba limitaciones de rendimiento y consumo:
- Bloqueos de intérprete por GIL (*Global Interpreter Lock*) y multihilo tradicional.
- Alto consumo de memoria RAM (80-120 MB en reposo).
- Sobrecarga de deserialización y sincronización.

La reescritura en **Rust** introduce:
- **Zero-cost abstractions** y concurrencia cooperativa sobre Tokio: huella de memoria < 15 MB RAM.
- **Seguridad de memoria estricta** con `#![deny(unsafe_code)]`.
- **Protección contra ataques de temporización** (*timing attacks*) usando `subtle::ConstantTimeEq` para validar el token Bearer.
- **Healthcheck nativo** integrado en el binario (`--healthcheck`) eliminando dependencias de shell o herramientas externas en el contenedor.

---

## 2. Arquitectura interna

El componente se divide en capas modulares desacopladas:

1. **Configuración (`config.rs`)**: Carga y valida variables de entorno (`Settings`, `DeviceConfig`).
2. **Dominio agronómico (`domain/`)**:
   - `crop.rs`: Perfiles basales de especies (Lechuga, Tomate, Fresa, Albahaca, Espinaca, Pimentón).
   - `limits.rs`: Límites de medición física de los sensores y funciones de acotamiento (*clamping*).
   - `actuator.rs`: Tipos de actuadores, duraciones legibles ("15 s", "10 min", "2 h") y desinencias de género en español.
   - `environment.rs`: Motor de simulación física con cálculo de objetivos, relajación exponencial, evaporación, riego y ruido gaussiano.
   - `weather.rs`: Cliente Open-Meteo con mapeo de códigos WMO y caché geográfica en memoria por cuadrícula con TTL de 10 minutos.
3. **Dispositivo simulado (`device.rs`)**: Conexión MQTT cliente mediante `rumqttc`, bucle de eventos asíncrono, suscripción a órdenes y publicación de telemetría.
4. **Gestor de cultivos (`pots.rs`)**: `PotManager` administra `Arc<RwLock<VirtualPot>>`, orquesta el reloj de publicación e interactúa con el servicio de clima.
5. **API de control HTTP (`api.rs`)**: Servidor Axum con rutas `/health`, `/v1/pots`, `/v1/places` y `/v1/weather`, protegido por autenticación Bearer en tiempo constante.

---

## 3. Modelo físico de simulación

El motor físico en `Environment::step(seconds, now)` actualiza las variables de estado hidropónico:
- **Temperatura y humedad**: Relajan suavemente hacia el ambiente objetivo; el ventilador (`FAN`) enfría (-0.35°C/min) y deshumidifica (-1.5%/min); el humidificador (`HUMIDIFIER`) aporta humedad (+2.5%/min).
- **Luz solar y artificial**: En modo `AUTO`, sigue una curva senoidal día/noche con desfase UTC. La lámpara ultravioleta (`UV_LIGHT`) suma radiación lumínica (+900 lx) y un ligero incremento térmico.
- **Humedad del sustrato**: Modela la evaporación en función de la temperatura y el factor de secado ambiental, compensada por el riego de la bomba (`WATER_PUMP`, +1.2%/s) o la lluvia en cultivos al aire libre.
- **Evolución química**: La dosificación de pH (`PH_DOSER`) reduce 0.12 unidades por segundo dosificado; el dosificador de nutrientes (`NUTRIENT_DOSER`) incrementa 70 ppm por segundo de dosis.

---

## 4. Pruebas y validación

El componente cuenta con una suite completa de pruebas unitarias e integración:
- `tests/test_environment.rs` (11 pruebas): Verificación de escalas de sensores, enfriamiento por ventilador, iluminación nocturna por UV, dosificaciones y filtrado de clima por exposición solar.
- `tests/test_device.rs` (5 pruebas): Contrato MQTT v1, validación de comandos, descarte de comandos inválidos y parsing de listas de dispositivos.
- `tests/test_virtual_pots.rs` (7 pruebas): Escenas climáticas, caché de coordenadas, protección de cultivos estáticos, autorización de la API de control y ciclo completo CRUD de cultivos virtuales.

Todas las 23 pruebas corren en menos de 0.05 segundos en Rust nativo.
