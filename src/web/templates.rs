//! # Templates Maud — HTML Server-Side Rendering
//!
//! Templates HTML renderizados em tempo de compilação usando o macro
//! [`maud`](https://maud.lambda.xyz/). Maud compila templates diretamente
//! em código Rust, resultando em rendering ~10x mais rápido que engines
//! runtime (Handlebars, Tera) e com **zero allocations** desnecessárias.
//!
//! ## Filosofia: HTMX + Maud = Hypermedia-Driven
//!
//! Em vez de SPA (React/Vue), usamos o padrão **Hypermedia-Driven**:
//! - Servidor retorna **HTML fragments** (não JSON)
//! - HTMX no frontend injeta fragments no DOM
//! - Zero JavaScript app-level (exceto grafo 3D)
//!
//! ## Templates Disponíveis
//!
//! | Função | Tipo | Descrição |
//! |--------|------|-----------|
//! | [`full_page()`] | Página completa | Chat + sidebar + grafo 3D |
//! | [`visualizador_page()`] | Página completa | Grafo full-screen + SSE |
//! | [`sidebar_content()`] | Fragment HTMX | Lista de conceitos ativos/fading |
//!
//! ## Layout Principal (`full_page`)
//!
//! ```text
//! ┌──────────────── nav-bar ────────────────────┐
//! │ CE │ Chat │ Visualizador │ Metodologia │ ● │
//! ├──────────────────────────┬──────────────────┤
//! │                          │ Grafo 3D / Lista │
//! │    Chat Messages         │   ┌──────────┐   │
//! │    ┌─────────────┐       │   │  Canvas   │   │
//! │    │ Boas-vindas │       │   │  WebGL    │   │
//! │    │ Mensagens   │       │   └──────────┘   │
//! │    │ Métricas    │       │                  │
//! │    └─────────────┘       │  Conceitos Ativos│
//! │                          │  Conceitos Fading│
//! ├──────────────────────────┴──────────────────┤
//! │ [📄 PDF] [🗑 Reset] [_______________][Send] │
//! └─────────────────────────────────────────────┘
//! ```

use maud::{html, Markup, PreEscaped, DOCTYPE};

use crate::core::KnowledgeBase;

/// Página principal do chat — layout completo com sidebar e grafo 3D.
///
/// Inclui:
/// - **Nav bar** com links para Chat, Visualizador, Metodologia
/// - **Chat panel** (esquerda) com mensagens e input
/// - **Sidebar** (direita) com tabs para Grafo 3D e lista de conceitos
/// - **Scripts**: HTMX, graph3d.js, e JavaScript inline para interatividade
///
/// ## JavaScript Inline
///
/// O script inline no final gerencia:
/// - Toggle entre views da sidebar (grafo/conhecimento)
/// - Auto-scroll do chat quando novas mensagens chegam (MutationObserver)
/// - Polling do status do modelo (/status) a cada 3s
/// - Refresh do grafo após cada mensagem enviada
/// - SSE listener para mostrar resultado de ingestão PDF no chat
pub fn full_page() -> Markup {
    html! {
        (DOCTYPE)
        html lang="pt-BR" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { "Cultivo Epistêmico — Knowledge Bank" }
                link rel="stylesheet" href="/assets/style.css";
                script src="/assets/htmx.min.js" {}
            }
            body {
                div class="app-shell" {
                    // Navigation Bar
                    nav class="nav-bar" {
                        a href="/" class="nav-brand" {
                            span class="nav-brand-icon" { "CE" }
                            span class="nav-brand-text" {
                                "Cultivo " em { "Epistêmico" }
                            }
                        }

                        div class="nav-links" {
                            a href="/" class="nav-link active" {
                                span class="nav-link-icon" { "💬" }
                                span class="nav-link-label" { "Chat" }
                            }
                            a href="/visualizador" class="nav-link" {
                                span class="nav-link-icon" { "◎" }
                                span class="nav-link-label" { "Visualizador" }
                            }
                            a href="/metodologia" class="nav-link" {
                                span class="nav-link-icon" { "📖" }
                                span class="nav-link-label" { "Metodologia" }
                            }
                        }

                        div class="nav-status" id="nav-status" {
                            span class="nav-status-dot loading" id="status-dot" {}
                            span id="status-text" { "carregando modelo..." }
                        }
                    }

                    // Main content
                    div class="app-container" {
                        // Chat panel (esquerda)
                        div class="chat-panel" {
                            div id="chat-messages" class="chat-messages" {
                                div class="message system-message welcome" {
                                    div class="message-content" {
                                        div class="welcome-title" {
                                            "Bem-vindo ao Cultivo Epistêmico"
                                        }
                                        p {
                                            "Compartilhe conhecimento e eu vou cristalizar conceitos, "
                                            "encontrar relações e fazer perguntas reflexivas."
                                        }
                                        div class="welcome-features" {
                                            span class="welcome-feature" {
                                                span class="welcome-feature-icon" { "🌱" }
                                                "Semeadura"
                                            }
                                            span class="welcome-feature" {
                                                span class="welcome-feature-icon" { "🔬" }
                                                "Inferência"
                                            }
                                            span class="welcome-feature" {
                                                span class="welcome-feature-icon" { "📄" }
                                                "Upload PDF"
                                            }
                                            span class="welcome-feature" {
                                                span class="welcome-feature-icon" { "🍂" }
                                                "Poda Natural"
                                            }
                                        }
                                    }
                                }
                            }

                            div class="chat-input-area" {
                                // Upload PDF
                                form id="upload-form"
                                    enctype="multipart/form-data"
                                    hx-post="/upload"
                                    hx-target="#chat-messages"
                                    hx-swap="beforeend"
                                    hx-encoding="multipart/form-data" {
                                    label class="upload-btn" {
                                        "📄 PDF"
                                        input type="file" name="pdf" accept=".pdf"
                                            style="display:none"
                                            onchange="this.form.requestSubmit()";
                                    }
                                }

                                // Reset KB button
                                button class="upload-btn"
                                    hx-post="/knowledge/reset"
                                    hx-target="#chat-messages"
                                    hx-swap="beforeend"
                                    hx-confirm="Tem certeza? Todos os conceitos e links serão removidos." {
                                    "🗑 Reset KB"
                                }

                                // Chat form
                                form id="chat-form"
                                    hx-post="/chat"
                                    hx-target="#chat-messages"
                                    hx-swap="beforeend"
                                    hx-on-after-request="this.reset()" {
                                    input type="text" name="message"
                                        placeholder="Compartilhe algo que aprendeu..."
                                        autocomplete="off"
                                        autofocus
                                        onkeydown="if(event.key==='Enter'){event.preventDefault();}";
                                    button type="submit" { "Enviar" }
                                }
                            }
                        }

                        // Knowledge sidebar (direita)
                        div class="sidebar" {
                            div class="sidebar-tabs" {
                                button #tab-graph class="sidebar-tab active"
                                    onclick="toggleSidebarView('graph')" {
                                    span class="sidebar-tab-icon" { "◎" }
                                    "Grafo 3D"
                                }
                                button #tab-knowledge class="sidebar-tab"
                                    onclick="toggleSidebarView('knowledge')" {
                                    span class="sidebar-tab-icon" { "◈" }
                                    "Conhecimento"
                                }
                            }

                            div #graph-view class="sidebar-view active" {
                                div class="graph-container" {
                                    canvas #graph3d class="graph-canvas" {}
                                    div #graphStats class="graph-stats" {}
                                }
                            }

                            div #knowledge-view class="sidebar-view" {
                                div class="sidebar-header" {
                                    h2 { "Conhecimento" }
                                }
                                div id="knowledge-sidebar"
                                    hx-get="/knowledge/sidebar"
                                    hx-trigger="load, every 10s"
                                    hx-swap="innerHTML" {
                                    div class="sidebar-loading" { "Carregando..." }
                                }
                            }
                        }
                    }
                }

                script src="/assets/graph3d.js" {}
                (PreEscaped(r#"<script>
var _graph3d = null;

function toggleSidebarView(view) {
  var graphView = document.getElementById('graph-view');
  var knowledgeView = document.getElementById('knowledge-view');
  var tabGraph = document.getElementById('tab-graph');
  var tabKnowledge = document.getElementById('tab-knowledge');

  if (view === 'graph') {
    graphView.classList.add('active');
    knowledgeView.classList.remove('active');
    tabGraph.classList.add('active');
    tabKnowledge.classList.remove('active');
    if (_graph3d) _graph3d._resize();
  } else {
    graphView.classList.remove('active');
    knowledgeView.classList.add('active');
    tabGraph.classList.remove('active');
    tabKnowledge.classList.add('active');
  }
}

document.addEventListener('DOMContentLoaded', function() {
  var canvas = document.getElementById('graph3d');
  var stats = document.getElementById('graphStats');
  if (canvas) {
    _graph3d = new Graph3D();
    _graph3d.init(canvas, stats);
  }

  // Auto-scroll: observe any new children added to chat-messages
  var msgs = document.getElementById('chat-messages');
  if (msgs) {
    var observer = new MutationObserver(function() {
      msgs.scrollTop = msgs.scrollHeight;
    });
    observer.observe(msgs, { childList: true, subtree: true });
  }

  // Poll model status
  function checkModelStatus() {
    fetch('/status')
      .then(function(r) { return r.json(); })
      .then(function(data) {
        var dot = document.getElementById('status-dot');
        var text = document.getElementById('status-text');
        if (data.ready) {
          dot.classList.remove('loading');
          text.textContent = 'pronto';
        } else {
          setTimeout(checkModelStatus, 3000);
        }
      })
      .catch(function() {
        setTimeout(checkModelStatus, 5000);
      });
  }
  checkModelStatus();
});

document.body.addEventListener('htmx:afterRequest', function(e) {
  var path = e.detail.pathInfo ? e.detail.pathInfo.requestPath : '';
  if (path === '/chat' || path === '/upload') {
    if (_graph3d) _graph3d.refresh();
  }
});

// SSE: listen for PDF ingestion completion to show result in chat
(function() {
  function fmtDur(ms) {
    if (ms < 1000) return ms + 'ms';
    if (ms < 60000) return (ms / 1000).toFixed(1) + 's';
    var m = Math.floor(ms / 60000);
    var s = ((ms % 60000) / 1000).toFixed(0);
    return m + 'm' + s + 's';
  }

  var es = new EventSource('/events');
  es.onmessage = function(e) {
    try {
      var ev = JSON.parse(e.data);
      if (ev.type === 'Completed') {
        var msgs = document.getElementById('chat-messages');
        if (!msgs) return;
        var div = document.createElement('div');
        div.className = 'message system-message pdf-result';
        var metricsHtml = '';
        if (ev.memory_used_mb !== undefined) {
          var kbSz = ev.kb_file_size_bytes < 1024*1024
            ? (ev.kb_file_size_bytes/1024).toFixed(1)+' KB'
            : (ev.kb_file_size_bytes/(1024*1024)).toFixed(1)+' MB';
          metricsHtml = '<br><span style="font-family:\'DM Mono\',monospace;font-size:12px;color:var(--bone)">' +
            '\u26a1 RAM ' + ev.memory_used_mb.toFixed(1) + ' MB' +
            ' | CPU ' + ev.cpu_active_cores + '/' + ev.cpu_total_cores +
            ' cores peak ' + ev.cpu_max_core_percent.toFixed(1) + '%' +
            ' | KB ' + kbSz +
            ' | ' + ev.gpu_name + ' ' + ev.gpu_cores + ' GPU cores ' +
            ev.gpu_utilization_pct + '% ' + ev.gpu_memory_mb.toFixed(0) + ' MB' +
            (ev.throughput ? ' | ' + ev.throughput : '') +
            '</span>';
        }
        div.innerHTML =
          '<div class="message-role">PDF Completo</div>' +
          '<div class="message-content">' +
            '\u{1f4c4} Ingestão finalizada: ' + ev.total_chunks + ' chunks \u2192 ' +
            ev.new_concepts + ' conceitos, ' + ev.new_links + ' links. ' +
            'KB: ' + ev.kb_concepts + ' conceitos, ' + ev.kb_links + ' links.<br>' +
            '<span style="font-family:\'DM Mono\',monospace;font-size:12px;color:var(--bone)">' +
            '\u23f1 Leitura: ' + fmtDur(ev.extract_ms) +
            ' | Ingestão: ' + fmtDur(ev.ingestion_ms) +
            ' | Total: ' + fmtDur(ev.total_ms) +
            '</span>' +
            metricsHtml +
          '</div>';
        msgs.appendChild(div);
        if (_graph3d) _graph3d.refresh();
      }
    } catch(err) {}
  };
})();
</script>"#))
            }
        }
    }
}

/// Página do Visualizador — grafo 3D em tela cheia + feed de atividade SSE.
///
/// Layout dedicado para monitoramento de ingestão de PDF em tempo real:
/// - **Grafo 3D** (esquerda) — Canvas WebGL com force-directed graph
/// - **Feed de atividade** (direita) — logs SSE + barra de progresso
/// - **Painel de propriedades** — detalhes de nó/aresta selecionado
///
/// Os scripts `graph3d.js` e `visualizador.js` são carregados
/// separadamente (não inline como na página principal).
pub fn visualizador_page() -> Markup {
    html! {
        (DOCTYPE)
        html lang="pt-BR" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { "Cultivo Epistêmico — Visualizador" }
                link rel="stylesheet" href="/assets/style.css";
            }
            body {
                div class="app-shell" {
                    // Navigation Bar
                    nav class="nav-bar" {
                        a href="/" class="nav-brand" {
                            span class="nav-brand-icon" { "CE" }
                            span class="nav-brand-text" {
                                "Cultivo " em { "Epistêmico" }
                            }
                        }

                        div class="nav-links" {
                            a href="/" class="nav-link" {
                                span class="nav-link-icon" { "💬" }
                                span class="nav-link-label" { "Chat" }
                            }
                            a href="/visualizador" class="nav-link active" {
                                span class="nav-link-icon" { "◎" }
                                span class="nav-link-label" { "Visualizador" }
                            }
                            a href="/metodologia" class="nav-link" {
                                span class="nav-link-icon" { "📖" }
                                span class="nav-link-label" { "Metodologia" }
                            }
                        }

                        div class="nav-status" id="nav-status" {
                            span class="nav-status-dot loading" id="status-dot" {}
                            span id="status-text" { "carregando modelo..." }
                        }
                    }

                    // Visualizador content
                    div class="visualizador-container" {
                        div class="visualizador-graph" {
                            canvas id="viz-graph3d" class="graph-canvas" {}
                            div id="vizGraphStats" class="graph-stats" {}
                        }

                        div class="activity-feed" {
                            // Tabs
                            div class="viz-panel-tabs" {
                                button id="tab-activity" class="viz-panel-tab active"
                                    onclick="switchVizTab('activity')" {
                                    "Atividade"
                                }
                                button id="tab-properties" class="viz-panel-tab"
                                    onclick="switchVizTab('properties')" {
                                    "Propriedades"
                                }
                                span class="activity-status" id="sse-status" { "desconectado" }
                            }

                            // Activity panel
                            div id="panel-activity" class="viz-panel-content active" {
                                div class="activity-progress" id="progress-container" style="display:none" {
                                    div class="progress-label" id="progress-label" { "Aguardando..." }
                                    div class="progress-bar" {
                                        div class="progress-fill" id="progress-fill" style="width:0%" {}
                                    }
                                }

                                div class="activity-log" id="activity-log" {
                                    div class="log-empty" {
                                        "Aguardando eventos de ingestão..."
                                    }
                                }
                            }

                            // Properties panel
                            div id="panel-properties" class="viz-panel-content" {
                                div id="properties-content" class="properties-empty" {
                                    div class="properties-empty-icon" { "◎" }
                                    p { "Clique em um conceito ou link no grafo para ver suas propriedades." }
                                }
                            }
                        }
                    }
                }

                script src="/assets/graph3d.js" {}
                script src="/assets/visualizador.js" {}
            }
        }
    }
}

/// Fragment HTMX da sidebar de conhecimento.
///
/// Renderiza a lista de conceitos da KB em duas seções:
/// - **✦ Ativos** — até 20 conceitos com energia alta (verde)
/// - **🍂 Esmaecendo** — até 10 conceitos em decay (amarelo/laranja)
///
/// Cada concept card mostra:
/// - Label + TruthValue (frequência, confiança)
/// - Barra de energia visual (CSS width %)
/// - Contagem de menções
/// - Botão "↑" para reforço via HTMX POST
///
/// Se a KB estiver vazia, exibe mensagem de boas-vindas.
pub fn sidebar_content(kb: &KnowledgeBase) -> Markup {
    let active = kb.active_concepts();
    let fading = kb.fading_concepts();

    html! {
        div class="kb-stats" {
            div class="stat" {
                span class="stat-value" { (kb.concept_count()) }
                span class="stat-label" { "Conceitos" }
            }
            div class="stat" {
                span class="stat-value" { (kb.link_count()) }
                span class="stat-label" { "Links" }
            }
        }

        @if !active.is_empty() {
            div class="sidebar-section" {
                h3 { "✦ Ativos" }
                @for concept in active.iter().take(20) {
                    div class="concept-card active" {
                        div class="concept-header" {
                            span class="concept-label" { (concept.label.clone()) }
                            span class="concept-truth" { (concept.truth.to_string()) }
                        }
                        div class="concept-meta" {
                            span class="energy-bar" {
                                span class="energy-fill"
                                    style=(format!("width: {}%", (concept.energy * 100.0) as u32)) {}
                            }
                            span class="mention-count" { "×" (concept.mention_count) }
                        }
                        button class="reinforce-btn"
                            hx-post=(format!("/knowledge/reinforce/{}", concept.id))
                            hx-target="#chat-messages"
                            hx-swap="beforeend" {
                            "↑"
                        }
                    }
                }
            }
        }

        @if !fading.is_empty() {
            div class="sidebar-section fading-section" {
                h3 { "🍂 Esmaecendo" }
                @for concept in fading.iter().take(10) {
                    div class="concept-card fading" {
                        div class="concept-header" {
                            span class="concept-label" { (concept.label.clone()) }
                            span class="concept-truth" { (concept.truth.to_string()) }
                        }
                        div class="concept-meta" {
                            span class="energy-bar" {
                                span class="energy-fill fading-fill"
                                    style=(format!("width: {}%", (concept.energy * 100.0) as u32)) {}
                            }
                        }
                        button class="reinforce-btn"
                            hx-post=(format!("/knowledge/reinforce/{}", concept.id))
                            hx-target="#chat-messages"
                            hx-swap="beforeend" {
                            "↑ Reforçar"
                        }
                    }
                }
            }
        }

        @if kb.concept_count() == 0 {
            div class="sidebar-empty" {
                div class="sidebar-empty-icon" { "🌿" }
                p { "Nenhum conceito ainda." }
                p class="hint" { "Envie uma mensagem para começar a cristalizar conhecimento." }
            }
        }
    }
}
