/**
 * Visualizador — SSE Event Listener + Graph3D full-screen + Activity Feed + Properties Panel
 * Connects to /events endpoint and displays real-time ingestion progress
 */
(function () {
  'use strict';

  var graph = null;
  var eventSource = null;
  var refreshTimer = null;
  var REFRESH_DEBOUNCE = 300;

  function init() {
    // Init Graph3D
    var canvas = document.getElementById('viz-graph3d');
    var stats = document.getElementById('vizGraphStats');
    if (canvas) {
      graph = new Graph3D();
      graph.onSelect = onGraphSelect;
      graph.init(canvas, stats);
    }

    // Check model status
    checkModelStatus();

    // Connect SSE
    connectSSE();
  }

  // --- Tab switching ---

  window.switchVizTab = function (tab) {
    var tabActivity = document.getElementById('tab-activity');
    var tabProperties = document.getElementById('tab-properties');
    var panelActivity = document.getElementById('panel-activity');
    var panelProperties = document.getElementById('panel-properties');

    if (tab === 'activity') {
      tabActivity.classList.add('active');
      tabProperties.classList.remove('active');
      panelActivity.classList.add('active');
      panelProperties.classList.remove('active');
    } else {
      tabActivity.classList.remove('active');
      tabProperties.classList.add('active');
      panelActivity.classList.remove('active');
      panelProperties.classList.add('active');
    }
  };

  // --- Graph selection → Properties panel ---

  function onGraphSelect(type, data) {
    var container = document.getElementById('properties-content');
    if (!container) return;

    if (!type || !data) {
      container.className = 'properties-empty';
      container.innerHTML =
        '<div class="properties-empty-icon">\u25CE</div>' +
        '<p>Clique em um conceito ou link no grafo para ver suas propriedades.</p>';
      return;
    }

    // Switch to properties tab
    switchVizTab('properties');

    if (type === 'node') {
      renderNodeProperties(container, data);
    } else if (type === 'edge') {
      renderEdgeProperties(container, data);
    }
  }

  function renderNodeProperties(el, node) {
    var stateLabel = {
      active: 'Ativo', dormant: 'Dormente', fading: 'Esmaecendo', archived: 'Arquivado'
    };
    var stateColor = {
      active: 'var(--seed)', dormant: 'var(--sun)', fading: 'var(--prune)', archived: 'var(--decay)'
    };

    var color = stateColor[node.state] || 'var(--ash)';
    var label = stateLabel[node.state] || node.state;

    // Find connected edges
    var edges = graph ? graph.getNodeEdges(node.id) : [];

    var html = '' +
      '<div class="prop-header">' +
        '<div class="prop-type-badge concept-badge">Conceito</div>' +
      '</div>' +
      '<div class="prop-title">' + escapeHtml(node.label) + '</div>' +
      '<div class="prop-id">' + escapeHtml(node.id) + '</div>' +

      '<div class="prop-section">' +
        '<div class="prop-section-title">Estado</div>' +
        '<div class="prop-row">' +
          '<span class="prop-label">Estado</span>' +
          '<span class="prop-value" style="color:' + color + '">' +
            '<span class="prop-state-dot" style="background:' + color + '"></span>' +
            label +
          '</span>' +
        '</div>' +
        '<div class="prop-row">' +
          '<span class="prop-label">Energia</span>' +
          '<span class="prop-value">' +
            '<span class="prop-energy-bar"><span class="prop-energy-fill" style="width:' + (node.energy * 100).toFixed(0) + '%;background:' + color + '"></span></span>' +
            '<span class="prop-energy-pct">' + (node.energy * 100).toFixed(1) + '%</span>' +
          '</span>' +
        '</div>' +
      '</div>' +

      '<div class="prop-section">' +
        '<div class="prop-section-title">Truth Value</div>' +
        '<div class="prop-row">' +
          '<span class="prop-label">Frequência</span>' +
          '<span class="prop-value mono">' + node.frequency.toFixed(4) + '</span>' +
        '</div>' +
        '<div class="prop-row">' +
          '<span class="prop-label">Confiança</span>' +
          '<span class="prop-value mono">' + node.confidence.toFixed(4) + '</span>' +
        '</div>' +
        '<div class="prop-row">' +
          '<span class="prop-label">Menções</span>' +
          '<span class="prop-value mono">' + node.mentionCount + '</span>' +
        '</div>' +
      '</div>';

    if (edges.length > 0) {
      html += '<div class="prop-section">' +
        '<div class="prop-section-title">Links (' + edges.length + ')</div>';
      for (var i = 0; i < edges.length; i++) {
        var edge = edges[i];
        var otherId = edge.source === node.id ? edge.target : edge.source;
        var otherNode = graph.nodeMap[otherId];
        var otherLabel = otherNode ? otherNode.label : otherId.substring(0, 8) + '...';
        var direction = edge.source === node.id ? '\u2192' : '\u2190';
        html += '<div class="prop-link-item" data-edge-id="' + escapeHtml(edge.id) + '">' +
          '<span class="prop-link-kind">' + escapeHtml(edge.kind) + '</span>' +
          '<span class="prop-link-dir">' + direction + '</span>' +
          '<span class="prop-link-target">' + escapeHtml(otherLabel) + '</span>' +
        '</div>';
      }
      html += '</div>';
    }

    el.className = 'properties-detail';
    el.innerHTML = html;

    // Make linked items clickable
    var linkItems = el.querySelectorAll('.prop-link-item');
    for (var li = 0; li < linkItems.length; li++) {
      linkItems[li].addEventListener('click', (function(edgeId) {
        return function() {
          var edgeData = null;
          for (var ei = 0; ei < graph.edges.length; ei++) {
            if (graph.edges[ei].id === edgeId) { edgeData = graph.edges[ei]; break; }
          }
          if (edgeData) {
            graph.selectedNode = null;
            graph.selectedEdge = edgeId;
            onGraphSelect('edge', edgeData);
          }
        };
      })(linkItems[li].getAttribute('data-edge-id')));
    }
  }

  function renderEdgeProperties(el, edge) {
    var srcNode = graph ? graph.nodeMap[edge.source] : null;
    var tgtNode = graph ? graph.nodeMap[edge.target] : null;
    var srcLabel = srcNode ? srcNode.label : edge.source.substring(0, 8) + '...';
    var tgtLabel = tgtNode ? tgtNode.label : edge.target.substring(0, 8) + '...';

    var html = '' +
      '<div class="prop-header">' +
        '<div class="prop-type-badge link-badge">Link</div>' +
      '</div>' +
      '<div class="prop-title">' + escapeHtml(srcLabel) + ' \u2192 ' + escapeHtml(tgtLabel) + '</div>' +
      '<div class="prop-id">' + escapeHtml(edge.id) + '</div>' +

      '<div class="prop-section">' +
        '<div class="prop-section-title">Relação</div>' +
        '<div class="prop-row">' +
          '<span class="prop-label">Tipo</span>' +
          '<span class="prop-value prop-kind-badge">' + escapeHtml(edge.kind) + '</span>' +
        '</div>' +
      '</div>' +

      '<div class="prop-section">' +
        '<div class="prop-section-title">Participantes</div>' +
        '<div class="prop-participant" data-node-id="' + escapeHtml(edge.source) + '">' +
          '<span class="prop-role">Sujeito</span>' +
          '<span class="prop-participant-label">' + escapeHtml(srcLabel) + '</span>' +
        '</div>' +
        '<div class="prop-participant" data-node-id="' + escapeHtml(edge.target) + '">' +
          '<span class="prop-role">Objeto</span>' +
          '<span class="prop-participant-label">' + escapeHtml(tgtLabel) + '</span>' +
        '</div>' +
      '</div>' +

      '<div class="prop-section">' +
        '<div class="prop-section-title">Truth Value</div>' +
        '<div class="prop-row">' +
          '<span class="prop-label">Frequência</span>' +
          '<span class="prop-value mono">' + edge.frequency.toFixed(4) + '</span>' +
        '</div>' +
        '<div class="prop-row">' +
          '<span class="prop-label">Confiança</span>' +
          '<span class="prop-value mono">' + edge.confidence.toFixed(4) + '</span>' +
        '</div>' +
        '<div class="prop-row">' +
          '<span class="prop-label">Energia</span>' +
          '<span class="prop-value mono">' + (edge.energy * 100).toFixed(1) + '%</span>' +
        '</div>' +
      '</div>';

    el.className = 'properties-detail';
    el.innerHTML = html;

    // Make participants clickable
    var participants = el.querySelectorAll('.prop-participant');
    for (var pi = 0; pi < participants.length; pi++) {
      participants[pi].addEventListener('click', (function(nodeId) {
        return function() {
          var nodeData = graph.nodeMap[nodeId];
          if (nodeData) {
            graph.selectedEdge = null;
            graph.selectedNode = nodeId;
            onGraphSelect('node', nodeData);
          }
        };
      })(participants[pi].getAttribute('data-node-id')));
    }
  }

  // --- Model status ---

  function checkModelStatus() {
    fetch('/status')
      .then(function (r) { return r.json(); })
      .then(function (data) {
        var dot = document.getElementById('status-dot');
        var text = document.getElementById('status-text');
        if (data.ready) {
          dot.classList.remove('loading');
          text.textContent = 'pronto';
        } else {
          setTimeout(checkModelStatus, 3000);
        }
      })
      .catch(function () {
        setTimeout(checkModelStatus, 5000);
      });
  }

  // --- SSE ---

  function connectSSE() {
    var statusEl = document.getElementById('sse-status');

    if (eventSource) {
      eventSource.close();
    }

    eventSource = new EventSource('/events');

    eventSource.onopen = function () {
      statusEl.textContent = 'conectado';
      statusEl.classList.add('connected');
    };

    eventSource.onerror = function () {
      statusEl.textContent = 'reconectando...';
      statusEl.classList.remove('connected');
    };

    eventSource.onmessage = function (e) {
      try {
        var event = JSON.parse(e.data);
        handleEvent(event);
      } catch (err) {
        // ignore parse errors
      }
    };
  }

  function handleEvent(event) {
    switch (event.type) {
      case 'Started':
        onStarted(event);
        break;
      case 'ChunkStarted':
        onChunkStarted(event);
        break;
      case 'ConceptCreated':
        onConceptCreated(event);
        break;
      case 'ConceptReinforced':
        onConceptReinforced(event);
        break;
      case 'LinkCreated':
        onLinkCreated(event);
        break;
      case 'ChunkCompleted':
        onChunkCompleted(event);
        break;
      case 'Completed':
        onCompleted(event);
        break;
      case 'Error':
        onError(event);
        break;
    }
  }

  // --- SSE Event Handlers ---

  function onStarted(ev) {
    // Switch to activity tab
    switchVizTab('activity');

    var log = document.getElementById('activity-log');
    log.innerHTML = '';

    var container = document.getElementById('progress-container');
    container.style.display = 'block';

    var label = document.getElementById('progress-label');
    label.textContent = 'Iniciando... ' + ev.total_chunks + ' chunks (' + formatBytes(ev.text_len) + ')';

    var fill = document.getElementById('progress-fill');
    fill.style.width = '0%';

    addLogEntry('chunk-info', '\uD83D\uDCC4', 'Ingestão iniciada: ' + ev.total_chunks + ' chunks');
  }

  function onChunkStarted(ev) {
    var label = document.getElementById('progress-label');
    label.textContent = 'Chunk ' + ev.chunk + '/' + ev.total + ' (' + ev.chars + ' chars)';
  }

  function onConceptCreated(ev) {
    addLogEntry('concept-new', '\uD83C\uDF31', 'Novo: ' + ev.label);
    scheduleGraphRefresh();
  }

  function onConceptReinforced(ev) {
    var simText = ev.similarity ? ' (sim=' + ev.similarity.toFixed(2) + ')' : '';
    addLogEntry('concept-reinforced', '\uD83C\uDF3F', ev.label + simText + ' \u2192 ' + (ev.energy * 100).toFixed(0) + '%');
    scheduleGraphRefresh();
  }

  function onLinkCreated(ev) {
    addLogEntry('link-created', '\uD83D\uDD17', ev.source_label + ' \u2192 ' + ev.target_label);
    scheduleGraphRefresh();
  }

  function onChunkCompleted(ev) {
    var pct = ((ev.chunk / ev.total) * 100).toFixed(0);
    var fill = document.getElementById('progress-fill');
    fill.style.width = pct + '%';

    var label = document.getElementById('progress-label');
    label.textContent = 'Chunk ' + ev.chunk + '/' + ev.total + ' completo (' + pct + '%)';

    if (ev.new_concepts > 0 || ev.new_links > 0) {
      addLogEntry('chunk-info', '\u2713', 'Chunk ' + ev.chunk + ': +' + ev.new_concepts + ' conceitos, +' + ev.new_links + ' links');
    }
  }

  function onCompleted(ev) {
    var fill = document.getElementById('progress-fill');
    fill.style.width = '100%';

    var label = document.getElementById('progress-label');
    label.textContent = 'Completo! (' + formatDuration(ev.total_ms) + ')';

    addLogEntry(
      'completed', '\u2705',
      'Ingestão completa: ' + ev.total_chunks + ' chunks \u2192 ' +
      ev.new_concepts + ' conceitos, ' + ev.new_links + ' links. ' +
      'KB: ' + ev.kb_concepts + ' conceitos, ' + ev.kb_links + ' links'
    );

    addLogEntry(
      'chunk-info', '\u23F1',
      'Tempo: leitura ' + formatDuration(ev.extract_ms) +
      ' | ingestão ' + formatDuration(ev.ingestion_ms) +
      ' | total ' + formatDuration(ev.total_ms)
    );

    // System metrics
    if (ev.memory_used_mb !== undefined) {
      var kbSize = ev.kb_file_size_bytes < 1024 * 1024
        ? (ev.kb_file_size_bytes / 1024).toFixed(1) + ' KB'
        : (ev.kb_file_size_bytes / (1024 * 1024)).toFixed(1) + ' MB';
      addLogEntry(
        'chunk-info', '\u26A1',
        'RAM ' + ev.memory_used_mb.toFixed(1) + ' MB' +
        ' | CPU ' + ev.cpu_active_cores + '/' + ev.cpu_total_cores +
        ' cores peak ' + ev.cpu_max_core_percent.toFixed(1) + '%' +
        ' | KB ' + kbSize +
        ' | ' + ev.gpu_name + ' ' + ev.gpu_cores + ' GPU cores ' +
        ev.gpu_utilization_pct + '% ' + ev.gpu_memory_mb.toFixed(0) + ' MB' +
        (ev.throughput ? ' | ' + ev.throughput : '')
      );
    }

    if (graph) graph.refresh();
  }

  function onError(ev) {
    addLogEntry('error', '\u26A0', ev.message);
  }

  // --- Helpers ---

  function addLogEntry(className, icon, text) {
    var log = document.getElementById('activity-log');

    var empty = log.querySelector('.log-empty');
    if (empty) empty.remove();

    var entry = document.createElement('div');
    entry.className = 'log-entry ' + className;
    entry.innerHTML = '<span class="log-icon">' + icon + '</span>' + escapeHtml(text);
    log.appendChild(entry);

    log.scrollTop = log.scrollHeight;
  }

  function scheduleGraphRefresh() {
    if (refreshTimer) clearTimeout(refreshTimer);
    refreshTimer = setTimeout(function () {
      if (graph) graph.refresh();
    }, REFRESH_DEBOUNCE);
  }

  function formatBytes(bytes) {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
    return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
  }

  function formatDuration(ms) {
    if (ms < 1000) return ms + 'ms';
    if (ms < 60000) return (ms / 1000).toFixed(1) + 's';
    var min = Math.floor(ms / 60000);
    var sec = ((ms % 60000) / 1000).toFixed(0);
    return min + 'm' + sec + 's';
  }

  function escapeHtml(text) {
    var div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  // Init on DOM ready
  document.addEventListener('DOMContentLoaded', init);
})();
