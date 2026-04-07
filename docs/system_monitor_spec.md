# Especificación Técnica: Sistema de Monitoreo de Recursos en Tiempo Real

Este documento detalla la arquitectura y el plan de implementación para integrar un monitor de recursos (CPU, RAM, GPU, VRAM) en la interfaz de Allux.

## 1. Arquitectura del Sistema

Para evitar latencia en la interfaz de usuario (UI), la recolección de métricas debe ejecutarse de forma asíncrona, separada del hilo de renderizado.

### Componentes
1.  **`MetricsState` (Modelo):** Una estructura de datos compartida mediante `Arc<RwLock<T>>` que almacena los valores actuales.
2.  **`MetricsCollector` (Worker):** Una tarea asíncrona (`tokio::spawn`) que se ejecuta en un loop infinito, consulta el hardware y actualiza el `MetricsState`.
3.  **`StatusBar/Dashboard` (Vista):** Componente de la TUI que lee el `MetricsState` en cada frame de renderizado y lo muestra al usuario.

## 2. Dependencias Necesarias (`Cargo.toml`)

Se requieren las siguientes librerías para obtener datos precisos y multiplataforma:

```toml
[dependencies]
# Para CPU y RAM (Multiplataforma)
sysinfo = "0.30" 

# Para GPU NVIDIA (Opcional, mediante feature flag)
# nvml-wrapper = { version = "0.10", optional = true }

# Para manejo de concurrencia
tokio = { version = "1", features = ["full"] }
```

## 3. Estructura de Datos Sugerida

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct SystemMetrics {
    pub cpu_usage: f32,
    pub ram_used: u64,
    pub ram_total: u64,
    pub gpu_usage: Option<f32>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
}

pub type SharedMetrics = Arc<RwLock<SystemMetrics>>;
```

##  4. Implementación del Collector (Prototipo)

```rust
use sysinfo::{CpuExt, System, SystemExt};
use std::time::Duration;
use tokio::time::sleep;

pub async fn start_metrics_collector(metrics: SharedMetrics) {
    let mut sys = System::new_all();
    
    loop {
        // 1. Actualizar datos de Sistema (CPU/RAM)
        sys.refresh_cpu();
        sys.refresh_memory();

        let cpu_usage = sys.global_cpu_info().cpu_usage();
        let ram_used = sys.used_memory();
        let ram_total = sys.total_memory();

        // 2. Actualizar el estado compartido
        {
            let mut w = metrics.write().await;
            w.cpu_usage = cpu_usage;
            w.ram_used = ram_used;
            w.ram_total = ram_total;
            // Nota: GPU requiere lógica específica para NVIDIA/AMD/Apple
        }

        // 3. Frecuencia de actualización (ej. 1 segundo)
        sleep(Duration::from_secs(1)).await;
    }
}
```

## 5. Estrategia de Visualización en la TUI

### Opción A: Barra de Estado (StatusBar)
Ideal para no ocupar espacio. Se coloca en la parte inferior de la pantalla.
`[ CPU: 12% | RAM: 4.2GB/16GB | GPU: 5% ]`

### Opción B: Panel de Diagnóstico (Dashboard)
Se activa mediante un comando o tecla (ej. `M` de Monitor).
- Usa barras de progreso (`indicatif`) para representar visualmente la carga.
- Color dinámico:
  - `0% - 60%`: Verde
  - `61% - 85%`: Amarillo
  - `86% - 100%`: Rojo

## 6. Consideraciones de Hardware

| Componente | Fuente de Datos | Complejidad |
| :--- | :--- | :--- |
| **CPU** | `sysinfo` (Standard) | Baja |
| **RAM** | `sysinfo` (Standard) | Baja |
| **GPU (NVIDIA)** | `nvml-wrapper` | Media |
| **GPU (Apple M1/M2)** | `IOKit` (macOS native) | Alta |
| **GPU (Linux/AMD)** | `/sys/class/drm/` | Media |

## 7. Plan de Acción
1. [ ] Crear módulo `src/monitor/mod.rs`.
2. [ ] Implementar el loop de recolección con `sysinfo`.
3. [ ] Integrar `SharedMetrics` en el loop principal de la aplicación.
4. [ ] Añadir componentes de visualización en la TUI.
5. [ ] (Opcional) Implementar soporte para GPU mediante condicionales de compilación.
