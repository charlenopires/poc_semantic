//! # Métricas do Sistema — Monitoramento em Tempo Real
//!
//! Módulo de coleta de métricas de hardware e processo, exibidas no
//! frontend após cada operação (chat ou ingestão de PDF).
//!
//! ## Métricas Coletadas
//!
//! | Categoria | Métrica | Fonte |
//! |-----------|---------|-------|
//! | RAM | Processo (MB) / Total (MB) | `sysinfo` |
//! | CPU | Cores ativos / Total / Pico por core | `sysinfo` |
//! | Disco | Tamanho do `data/kb.json` | `std::fs::metadata` |
//! | GPU | Nome, cores, utilização%, memória MB | IOKit (macOS) |
//! | Throughput | chars/s (opcional) | Calculado externamente |
//!
//! ## GPU no macOS — IOKit + AGXAccelerator
//!
//! No macOS, informações de GPU Apple Silicon são obtidas via IOKit,
//! acessando o driver `AGXAccelerator` e lendo `PerformanceStatistics`.
//! Isso é feito via FFI (Foreign Function Interface) com chamadas C
//! diretas ao framework IOKit do macOS.
//!
//! ```text
//! IOKit Registry
//! └── AGXAccelerator
//!     ├── model → "Apple M1 Pro" (nome da GPU)
//!     ├── gpu-core-count → 16 (núcleos GPU)
//!     └── PerformanceStatistics
//!         ├── Device Utilization % → 42 (uso da GPU)
//!         └── In use system memory → 1073741824 (RAM GPU em bytes)
//! ```
//!
//! ## Estado Persistente (System singleton)
//!
//! A lib `sysinfo` precisa de um baseline anterior para calcular
//! deltas de CPU. Por isso, mantemos uma única instância [`System`]
//! via [`OnceLock`] + [`Mutex`], reutilizada em todas as coletas.

use std::path::Path;
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};

// ─── System singleton (baseline para cálculo de CPU) ─────────────
// CPU usage requer um snapshot anterior para calcular delta.
// Mantemos uma única instância System para toda a vida do processo.

/// Singleton da instância [`System`] da lib `sysinfo`.
///
/// Inicializado na primeira chamada com `refresh_cpu_usage()` como
/// baseline. Chamadas subsequentes calculam deltas a partir deste.
static SYS: OnceLock<Mutex<System>> = OnceLock::new();

/// Retorna referência ao singleton System, inicializando se necessário.
fn system() -> &'static Mutex<System> {
    SYS.get_or_init(|| {
        let mut s = System::new();
        s.refresh_cpu_usage(); // baseline para deltas futuros
        Mutex::new(s)
    })
}

// ─── GPU macOS via IOKit FFI ─────────────────────────────────────
// Acessa AGXAccelerator (driver GPU Apple Silicon) via IOKit para
// obter nome, cores, utilização e memória da GPU.

/// Módulo condicional para coleta de métricas de GPU no macOS.
///
/// Usa FFI direto com IOKit para acessar `AGXAccelerator` e ler
/// `PerformanceStatistics`. Compilado apenas em `target_os = "macos"`.
///
/// ## Funções IOKit utilizadas
///
/// | Função | Propósito |
/// |--------|-----------|
/// | `IOServiceMatching` | Cria dicionário de busca por classe |
/// | `IOServiceGetMatchingServices` | Encontra serviços matching |
/// | `IOIteratorNext` | Itera sobre resultados |
/// | `IORegistryEntryCreateCFProperties` | Lê propriedades do dispositivo |
/// | `IOObjectRelease` | Libera referências IOKit |
#[cfg(target_os = "macos")]
mod gpu_macos {
    use core_foundation::base::TCFType;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::CFRelease;
    use core_foundation_sys::dictionary::{
        CFDictionaryGetValueIfPresent, CFDictionaryRef, CFMutableDictionaryRef,
    };
    use core_foundation_sys::number::CFNumberRef;
    use core_foundation_sys::string::CFStringRef;
    use std::ffi::CString;
    use std::os::raw::c_void;
    use std::ptr;

    // FFI direta com IOKit — sem wrapper Rust disponível para estas funções
    extern "C" {
        /// Cria dicionário de matching para buscar serviços IOKit por classe.
        fn IOServiceMatching(name: *const i8) -> CFMutableDictionaryRef;
        /// Busca serviços que correspondem ao dicionário de matching.
        fn IOServiceGetMatchingServices(
            main_port: u32,
            matching: CFDictionaryRef,
            existing: *mut u32,
        ) -> i32;
        /// Avança o iterador para o próximo serviço encontrado.
        fn IOIteratorNext(iterator: u32) -> u32;
        /// Lê todas as propriedades de um registro IOKit como CFDictionary.
        fn IORegistryEntryCreateCFProperties(
            entry: u32,
            properties: *mut CFMutableDictionaryRef,
            allocator: *const c_void,
            options: u32,
        ) -> i32;
        /// Decrementa a contagem de referência de um objeto IOKit.
        fn IOObjectRelease(obj: u32) -> u32;
    }

    /// Extrai um valor `i64` de um CFDictionary por chave string.
    ///
    /// Retorna `None` se a chave não existir ou o valor não for numérico.
    fn dict_get_i64(dict: CFDictionaryRef, key: &str) -> Option<i64> {
        unsafe {
            let cf_key = CFString::new(key);
            let mut value: *const c_void = ptr::null();
            if CFDictionaryGetValueIfPresent(dict, cf_key.as_CFTypeRef(), &mut value) != 0
                && !value.is_null()
            {
                CFNumber::wrap_under_get_rule(value as CFNumberRef).to_i64()
            } else {
                None
            }
        }
    }

    /// Extrai uma String de um CFDictionary por chave.
    ///
    /// Retorna `None` se a chave não existir ou o valor não for string.
    fn dict_get_string(dict: CFDictionaryRef, key: &str) -> Option<String> {
        unsafe {
            let cf_key = CFString::new(key);
            let mut value: *const c_void = ptr::null();
            if CFDictionaryGetValueIfPresent(dict, cf_key.as_CFTypeRef(), &mut value) != 0
                && !value.is_null()
            {
                Some(CFString::wrap_under_get_rule(value as CFStringRef).to_string())
            } else {
                None
            }
        }
    }

    /// Extrai um sub-dicionário de um CFDictionary por chave.
    ///
    /// Usado para acessar `PerformanceStatistics` dentro das propriedades
    /// do AGXAccelerator.
    fn dict_get_dict(dict: CFDictionaryRef, key: &str) -> Option<CFDictionaryRef> {
        unsafe {
            let cf_key = CFString::new(key);
            let mut value: *const c_void = ptr::null();
            if CFDictionaryGetValueIfPresent(dict, cf_key.as_CFTypeRef(), &mut value) != 0
                && !value.is_null()
            {
                Some(value as CFDictionaryRef)
            } else {
                None
            }
        }
    }

    /// Informações da GPU Apple Silicon.
    #[derive(Clone, Debug)]
    pub struct GpuInfo {
        /// Nome do modelo (ex: "Apple M1 Pro").
        pub name: String,
        /// Número de cores GPU.
        pub cores: u32,
        /// Porcentagem de utilização (0-100%).
        pub utilization_pct: u32,
        /// Memória de sistema em uso pela GPU (MB).
        pub memory_mb: f64,
    }

    /// Consulta informações da GPU via IOKit.
    ///
    /// ## Algoritmo
    ///
    /// 1. Busca serviço `AGXAccelerator` no IOKit Registry
    /// 2. Lê propriedades do dispositivo como CFDictionary
    /// 3. Extrai `model`, `gpu-core-count` do nível raiz
    /// 4. Extrai `Device Utilization %` e `In use system memory`
    ///    do sub-dicionário `PerformanceStatistics`
    /// 5. Libera todas as referências IOKit
    ///
    /// Retorna `None` se AGXAccelerator não for encontrado (ex: VM).
    pub fn query() -> Option<GpuInfo> {
        unsafe {
            let class_name = CString::new("AGXAccelerator").ok()?;
            let matching = IOServiceMatching(class_name.as_ptr());
            if matching.is_null() {
                return None;
            }

            let mut iterator: u32 = 0;
            if IOServiceGetMatchingServices(0, matching as CFDictionaryRef, &mut iterator) != 0 {
                return None;
            }

            let entry = IOIteratorNext(iterator);
            if entry == 0 {
                IOObjectRelease(iterator);
                return None;
            }

            let mut props: CFMutableDictionaryRef = ptr::null_mut();
            let kr = IORegistryEntryCreateCFProperties(entry, &mut props, ptr::null(), 0);
            IOObjectRelease(entry);
            IOObjectRelease(iterator);

            if kr != 0 || props.is_null() {
                return None;
            }

            let dict = props as CFDictionaryRef;

            // Nível raiz: modelo e contagem de cores
            let name = dict_get_string(dict, "model").unwrap_or_else(|| "Apple GPU".into());
            let cores = dict_get_i64(dict, "gpu-core-count").unwrap_or(0) as u32;

            // Sub-dicionário PerformanceStatistics: utilização e memória
            let (utilization_pct, in_use_memory) =
                if let Some(perf) = dict_get_dict(dict, "PerformanceStatistics") {
                    (
                        dict_get_i64(perf, "Device Utilization %").unwrap_or(0) as u32,
                        dict_get_i64(perf, "In use system memory").unwrap_or(0) as u64,
                    )
                } else {
                    (0, 0)
                };

            // Libera dicionário de propriedades (evita memory leak)
            CFRelease(props as *const c_void);

            Some(GpuInfo {
                name,
                cores,
                utilization_pct,
                memory_mb: in_use_memory as f64 / (1024.0 * 1024.0),
            })
        }
    }
}

// ─── ProcessMetrics ──────────────────────────────────────────────

/// Snapshot completo de métricas do sistema e processo.
///
/// Serializado como JSON e enviado ao frontend via SSE (no evento
/// `IngestionEvent::Completed`) e no endpoint `/api/metrics`.
///
/// ## Campos
///
/// | Campo | Unidade | Fonte |
/// |-------|---------|-------|
/// | `memory_used_mb` | MB | sysinfo (processo) |
/// | `memory_total_mb` | MB | sysinfo (sistema) |
/// | `cpu_active_cores` | count | cores com uso > 1% |
/// | `cpu_max_core_percent` | % | maior uso individual |
/// | `cpu_total_cores` | count | total lógico |
/// | `kb_file_size_bytes` | bytes | `data/kb.json` |
/// | `gpu_*` | variado | IOKit (macOS) |
/// | `throughput` | chars/s | calculado externamente |
#[derive(Clone, Debug, Serialize)]
pub struct ProcessMetrics {
    /// Memória RSS do processo em MB.
    pub memory_used_mb: f64,
    /// Memória total do sistema em MB.
    pub memory_total_mb: f64,
    /// Número de cores CPU com uso > 1% (indicam atividade real).
    pub cpu_active_cores: usize,
    /// Maior uso individual de CPU entre todos os cores (%).
    pub cpu_max_core_percent: f32,
    /// Total de cores lógicos (inclui hyperthreading).
    pub cpu_total_cores: usize,
    /// Tamanho do arquivo `data/kb.json` em bytes (0 se não existir).
    pub kb_file_size_bytes: u64,
    /// Nome da GPU (ex: "Apple M1 Pro" ou "N/A").
    pub gpu_name: String,
    /// Número de cores GPU.
    pub gpu_cores: u32,
    /// Utilização da GPU em porcentagem.
    pub gpu_utilization_pct: u32,
    /// Memória da GPU em uso (MB).
    pub gpu_memory_mb: f64,
    /// Throughput de processamento (ex: "1500 chars/s"), se disponível.
    pub throughput: Option<String>,
}

/// Coleta um snapshot de métricas do sistema e processo.
///
/// ## Sequência de Coleta
///
/// ```text
/// 1. Adquire lock do System singleton
/// 2. Refresh RAM, CPU, processo → sysinfo
/// 3. Libera lock (antes de IOKit para não segurar Mutex)
/// 4. Tamanho do arquivo KB → std::fs
/// 5. GPU → IOKit FFI (macOS) ou valores padrão
/// ```
///
/// ## Parâmetros
///
/// - `throughput` — throughput calculado externamente (ex: "1500 chars/s").
///   Passado como `None` quando não é uma operação de processamento.
pub fn collect_metrics(throughput: Option<String>) -> ProcessMetrics {
    let pid = Pid::from_u32(std::process::id());

    // Fase 1: sysinfo — RAM, CPU, processo
    let mut sys = system().lock();
    sys.refresh_memory();
    sys.refresh_cpu_usage();
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), false);

    // Memória do processo (RSS — Resident Set Size)
    let memory_used_mb = sys
        .process(pid)
        .map(|p| p.memory() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);
    let memory_total_mb = sys.total_memory() as f64 / (1024.0 * 1024.0);

    // CPU per-core — identifica cores ativos e pico
    let cpus = sys.cpus();
    let cpu_total_cores = cpus.len();
    let cpu_active_cores = cpus.iter().filter(|c| c.cpu_usage() > 1.0).count();
    let cpu_max_core_percent = cpus
        .iter()
        .map(|c| c.cpu_usage())
        .fold(0.0f32, f32::max);

    drop(sys); // Libera o Mutex ANTES de chamadas IOKit

    // Fase 2: Tamanho do arquivo KB
    let kb_file_size_bytes = Path::new("data/kb.json")
        .metadata()
        .map(|m| m.len())
        .unwrap_or(0);

    // Fase 3: GPU (macOS via IOKit, fallback para "N/A")
    #[cfg(target_os = "macos")]
    let (gpu_name, gpu_cores, gpu_utilization_pct, gpu_memory_mb) = match gpu_macos::query() {
        Some(g) => (g.name, g.cores, g.utilization_pct, g.memory_mb),
        None => ("Apple GPU (N/A)".into(), 0, 0, 0.0),
    };

    #[cfg(not(target_os = "macos"))]
    let (gpu_name, gpu_cores, gpu_utilization_pct, gpu_memory_mb) =
        ("N/A".into(), 0u32, 0u32, 0.0f64);

    ProcessMetrics {
        memory_used_mb,
        memory_total_mb,
        cpu_active_cores,
        cpu_max_core_percent,
        cpu_total_cores,
        kb_file_size_bytes,
        gpu_name,
        gpu_cores,
        gpu_utilization_pct,
        gpu_memory_mb,
        throughput,
    }
}

impl ProcessMetrics {
    /// Gera uma linha de sumário para exibição no chat.
    ///
    /// Formato: `"42ms | RAM 150.3 MB | CPU 4/8 cores peak 85.2% | KB 1.2 MB | Apple M1 Pro 16 GPU cores 42% 256 MB | 1500 chars/s"`
    ///
    /// O tamanho do arquivo KB é formatado automaticamente em B, KB, ou MB
    /// conforme o tamanho.
    pub fn summary_line(&self, elapsed_ms: u64) -> String {
        // Formata o tamanho do arquivo KB em unidade humana
        let kb_size = if self.kb_file_size_bytes < 1024 {
            format!("{} B", self.kb_file_size_bytes)
        } else if self.kb_file_size_bytes < 1024 * 1024 {
            format!("{:.1} KB", self.kb_file_size_bytes as f64 / 1024.0)
        } else {
            format!(
                "{:.1} MB",
                self.kb_file_size_bytes as f64 / (1024.0 * 1024.0)
            )
        };

        // Throughput opcional (só aparece em operações de processamento)
        let throughput_part = match &self.throughput {
            Some(t) => format!(" | {}", t),
            None => String::new(),
        };

        format!(
            "{}ms | RAM {:.1} MB | CPU {}/{} cores peak {:.1}% | KB {} | {} {} GPU cores {}% {:.0} MB{}",
            elapsed_ms,
            self.memory_used_mb,
            self.cpu_active_cores,
            self.cpu_total_cores,
            self.cpu_max_core_percent,
            kb_size,
            self.gpu_name,
            self.gpu_cores,
            self.gpu_utilization_pct,
            self.gpu_memory_mb,
            throughput_part,
        )
    }
}
