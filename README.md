# 🌱 Cultivo Epistêmico — Semantic Chat

> **Proof of Concept** de uma base de conhecimento semântica em Rust, inspirada no modelo biológico de cultivo de plantas e na lógica não-axiomática NARS.

O sistema transforma linguagem natural em uma rede de conceitos interconectados, aprende relações, realiza inferências lógicas e faz perguntas reflexivas — tudo em tempo real através de uma interface web interativa.

---

## ✨ Funcionalidades

| Funcionalidade | Descrição |
|---|---|
| 🌱 **Semeadura** | Extrai conceitos do texto do usuário e os cristaliza na base de conhecimento |
| 🔬 **Fotossíntese** | Motor de inferência NARS realiza dedução e indução sobre os conceitos |
| 🌿 **Germinação** | Gera perguntas reflexivas para conceitos com alta energia e baixa confiança |
| 🍂 **Poda** | Decai energia de conceitos não reforçados ao longo do tempo |
| 📄 **Ingestão PDF** | Extrai texto de PDFs, chunka e alimenta a KB com paralelismo |
| 📊 **Visualização 3D** | Grafo 3D interativo dos conceitos e suas relações em canvas |
| ⚡ **Métricas** | Monitoramento em tempo real de CPU, RAM, GPU e throughput |

## 🏗️ Stack Tecnológica

| Camada | Tecnologia |
|---|---|
| Linguagem | **Rust** (2021 edition) |
| Web Framework | **axum** 0.8 + **tower-http** |
| Templates | **maud** (HTML type-safe) |
| Frontend | **HTMX** + Vanilla JS + Canvas 3D |
| ML / Embeddings | **candle** (BERTimbau `neuralmind/bert-base-portuguese-cased`) |
| Lógica | **NARS** (Non-Axiomatic Reasoning System) |
| PDF | **pdf-extract** |
| Streaming | **SSE** (Server-Sent Events) via tokio broadcast |
| Paralelismo | **rayon** (data-parallel) + **tokio** (async) |
| Serialização | **serde** / **serde_json** |
| Métricas | **sysinfo** + IOKit (macOS GPU) |

## 🚀 Como Executar

### Pré-requisitos

- **Rust** 1.75+ (recomendado: stable mais recente)
- **macOS** com Apple Silicon (para aceleração Metal) ou qualquer OS com CPU
- ~400 MB de espaço para download do modelo BERTimbau (primeira execução)

### Compilação e Execução

```bash
# Clonar o repositório
git clone <url-do-repo>
cd semantic_chat

# Compilar e executar
cargo run

# Ou com logs detalhados
RUST_LOG=debug cargo run
```

O servidor inicia imediatamente em **http://localhost:3000** enquanto o modelo BERTimbau é carregado em background.

### Testes

```bash
cargo test
```

### Documentação Rust

```bash
cargo doc --no-deps --open
```

## 📁 Estrutura do Projeto

```
semantic_chat/
├── src/
│   ├── main.rs              # Ponto de entrada — inicializa servidor e modelo
│   ├── core/                # Tipos fundamentais do domínio
│   │   ├── mod.rs           # Re-exports do módulo core
│   │   ├── truth_value.rs   # TruthValue NARS (frequência, confiança)
│   │   ├── concept.rs       # Conceito — unidade atômica de conhecimento
│   │   ├── link.rs          # Link — relação N-ária entre conceitos
│   │   └── knowledge_base.rs # KnowledgeBase — contêiner de conceitos e links
│   ├── inference/           # Motor de inferência lógica
│   │   ├── mod.rs           # Re-exports do módulo inference
│   │   └── rules.rs         # Regras NARS: dedução, indução
│   ├── nlu/                 # Pipeline de compreensão de linguagem natural
│   │   ├── mod.rs           # NluPipeline — orquestra todo o processamento
│   │   ├── embedder.rs      # Embedder BERTimbau via candle
│   │   ├── extractor.rs     # Extrator de entidades por heurísticas
│   │   ├── intent.rs        # Classificador de intenção do usuário
│   │   └── question.rs      # Gerador de perguntas reflexivas
│   ├── orchestrator.rs      # Orquestrador do ciclo de cultivo epistêmico
│   ├── pdf.rs               # Processamento e ingestão de PDF
│   ├── persistence.rs       # Persistência da KB em JSON
│   ├── metrics.rs           # Coleta de métricas de sistema
│   └── web/                 # Camada web
│       ├── mod.rs           # Router axum e definição de rotas
│       ├── state.rs         # Estado compartilhado da aplicação
│       ├── handlers.rs      # Handlers HTTP (chat, upload, API)
│       ├── events.rs        # Tipos de eventos SSE
│       └── templates.rs     # Templates HTML com maud
├── assets/                  # Arquivos estáticos (CSS, JS, HTMX)
│   ├── style.css            # Estilos da interface
│   ├── htmx.min.js          # HTMX biblioteca
│   ├── graph3d.js           # Renderização 3D do grafo em canvas
│   └── visualizador.js      # Lógica do visualizador SSE
├── data/                    # Dados persistidos (gerado em runtime)
│   └── kb.json              # Base de conhecimento serializada
├── docs/                    # Documentação detalhada em PT-BR
│   ├── visao-geral.md       # Visão geral do projeto
│   ├── arquitetura.md       # Arquitetura do sistema
│   ├── nars-logica.md       # Lógica NARS explicada
│   ├── nlu-pipeline.md      # Pipeline NLU e BERTimbau
│   ├── base-conhecimento.md # Base de Conhecimento
│   ├── inferencia.md        # Motor de Inferência
│   ├── orquestrador.md      # Ciclo de Cultivo Epistêmico
│   ├── web-interface.md     # Interface Web
│   ├── pdf-ingestion.md     # Ingestão de PDF
│   └── como-executar.md     # Guia de execução
├── Cargo.toml               # Manifest do projeto Rust
└── Cargo.lock               # Lock de dependências
```

## 📚 Documentação

A documentação completa em Português Brasileiro está disponível em `docs/`:

| Documento | Descrição |
|---|---|
| [📋 Visão Geral](docs/visao-geral.md) | O que é o projeto, motivação e metáfora biológica |
| [🏗️ Arquitetura](docs/arquitetura.md) | Arquitetura do sistema e fluxo de dados |
| [🧠 Lógica NARS](docs/nars-logica.md) | TruthValue, frequency/confidence, regras de inferência |
| [🗣️ Pipeline NLU](docs/nlu-pipeline.md) | BERTimbau, embeddings, extração de entidades, classificação de intent |
| [📦 Base de Conhecimento](docs/base-conhecimento.md) | Concept, Link, KnowledgeBase e ciclo de vida |
| [🔬 Inferência](docs/inferencia.md) | Dedução, Indução e como o sistema raciocina |
| [🌱 Orquestrador](docs/orquestrador.md) | Ciclo de cultivo: semeadura→fotossíntese→germinação→poda |
| [🌐 Interface Web](docs/web-interface.md) | Axum, rotas HTTP, HTMX, SSE e templates maud |
| [📄 Ingestão PDF](docs/pdf-ingestion.md) | Como PDFs são processados e alimentam a KB |
| [🚀 Como Executar](docs/como-executar.md) | Guia completo de pré-requisitos, compilação e execução |

## 🗺️ Arquitetura (Visão Macro)

```
┌──────────────┐     ┌───────────────┐     ┌──────────────────┐
│   Frontend   │────▶│   Web Layer   │────▶│   Orchestrator   │
│  HTMX + JS   │◀────│  axum + maud  │◀────│ Ciclo Epistêmico │
└──────────────┘ SSE └───────────────┘     └──────────────────┘
                                                    │
                                 ┌──────────────────┼──────────────────┐
                                 ▼                  ▼                  ▼
                          ┌────────────┐    ┌──────────────┐   ┌────────────┐
                          │    NLU     │    │  Inference   │   │    PDF     │
                          │  Pipeline  │    │   Engine     │   │  Ingestion │
                          │ BERTimbau  │    │  NARS Rules  │   │   rayon    │
                          └────────────┘    └──────────────┘   └────────────┘
                                 │                  │
                                 ▼                  ▼
                          ┌──────────────────────────────┐
                          │       Knowledge Base         │
                          │  Concepts + Links + Index    │
                          │      (in-memory + JSON)      │
                          └──────────────────────────────┘
```

## 📄 Licença

Proof of Concept — uso interno e educacional.
