/**
 * Graph3D — 3D Knowledge Graph Visualization Engine
 * Canvas 2D + manual perspective projection
 */
(function () {
  'use strict';

  var FOV = 400;
  var DAMPING = 0.88;
  var REPULSION = 800;
  var ATTRACTION = 0.015;
  var CENTER_PULL = 0.002;
  var MIN_ZOOM = 0.3;
  var MAX_ZOOM = 3.0;
  var POLL_INTERVAL = 10000;

  // State colors matching CSS variables
  var STATE_COLORS = {
    active:   '#4ade80',
    dormant:  '#fbbf24',
    fading:   '#f87171',
    archived: '#6b7280'
  };

  var LINK_COLORS = {
    'é um':         '#4ade80',
    '≈':            '#22d3ee',
    '⇒':            '#a78bfa',
    '⇔':            '#a78bfa',
    'parte de':     '#fb923c',
    'tem':          '#fbbf24',
    'instância de': '#4ade80',
    'catalisa':     '#22d3ee',
    'inibe':        '#f87171'
  };

  function Graph3D() {
    this.canvas = null;
    this.ctx = null;
    this.nodes = [];
    this.edges = [];
    this.nodeMap = {};
    this.width = 0;
    this.height = 0;
    this.rotY = 0.3;
    this.rotX = 0.2;
    this.zoom = 1.0;
    this.dragging = false;
    this.lastMouse = null;
    this.hoveredNode = null;
    this.hoveredEdge = null;
    this.selectedNode = null;
    this.selectedEdge = null;
    this.animId = null;
    this.pollTimer = null;
    this.statsEl = null;
    this.onSelect = null; // callback(type, data) — type: 'node'|'edge'|null
    this._bound = {};
  }

  Graph3D.prototype.init = function (canvas, statsEl) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.statsEl = statsEl || null;
    this._resize();

    var self = this;

    this._bound.resize = function () { self._resize(); };
    this._bound.mousedown = function (e) { self._onMouseDown(e); };
    this._bound.mousemove = function (e) { self._onMouseMove(e); };
    this._bound.mouseup = function () { self._onMouseUp(); };
    this._bound.wheel = function (e) { self._onWheel(e); };
    this._bound.click = function (e) { self._onClick(e); };
    this._bound.touchstart = function (e) { self._onTouchStart(e); };
    this._bound.touchmove = function (e) { self._onTouchMove(e); };
    this._bound.touchend = function () { self._onMouseUp(); };

    window.addEventListener('resize', this._bound.resize);
    canvas.addEventListener('mousedown', this._bound.mousedown);
    canvas.addEventListener('mousemove', this._bound.mousemove);
    canvas.addEventListener('mouseup', this._bound.mouseup);
    canvas.addEventListener('mouseleave', this._bound.mouseup);
    canvas.addEventListener('wheel', this._bound.wheel, { passive: false });
    canvas.addEventListener('click', this._bound.click);
    canvas.addEventListener('touchstart', this._bound.touchstart, { passive: false });
    canvas.addEventListener('touchmove', this._bound.touchmove, { passive: false });
    canvas.addEventListener('touchend', this._bound.touchend);

    this.refresh();
    this._startLoop();
    this._startPolling();
  };

  Graph3D.prototype.destroy = function () {
    if (this.animId) {
      cancelAnimationFrame(this.animId);
      this.animId = null;
    }
    if (this.pollTimer) {
      clearInterval(this.pollTimer);
      this.pollTimer = null;
    }
    if (this.canvas) {
      var c = this.canvas;
      c.removeEventListener('mousedown', this._bound.mousedown);
      c.removeEventListener('mousemove', this._bound.mousemove);
      c.removeEventListener('mouseup', this._bound.mouseup);
      c.removeEventListener('mouseleave', this._bound.mouseup);
      c.removeEventListener('wheel', this._bound.wheel);
      c.removeEventListener('click', this._bound.click);
      c.removeEventListener('touchstart', this._bound.touchstart);
      c.removeEventListener('touchmove', this._bound.touchmove);
      c.removeEventListener('touchend', this._bound.touchend);
    }
    window.removeEventListener('resize', this._bound.resize);
  };

  Graph3D.prototype.refresh = function () {
    var self = this;
    fetch('/knowledge/graph')
      .then(function (r) { return r.json(); })
      .then(function (data) { self._updateData(data); })
      .catch(function () { /* silent */ });
  };

  // --- Data ---

  Graph3D.prototype._updateData = function (data) {
    var existingMap = this.nodeMap;
    var newMap = {};
    var nodes = [];
    var spread = 120;

    for (var i = 0; i < data.concepts.length; i++) {
      var c = data.concepts[i];
      var existing = existingMap[c.id];
      var node;
      if (existing) {
        node = existing;
        node.label = c.label;
        node.frequency = c.frequency;
        node.confidence = c.confidence;
        node.energy = c.energy;
        node.state = c.state;
        node.mentionCount = c.mention_count;
      } else {
        node = {
          id: c.id,
          label: c.label,
          frequency: c.frequency,
          confidence: c.confidence,
          energy: c.energy,
          state: c.state,
          mentionCount: c.mention_count,
          x: (Math.random() - 0.5) * spread,
          y: (Math.random() - 0.5) * spread,
          z: (Math.random() - 0.5) * spread,
          vx: 0, vy: 0, vz: 0
        };
      }
      nodes.push(node);
      newMap[c.id] = node;
    }

    var edges = [];
    for (var j = 0; j < data.links.length; j++) {
      var l = data.links[j];
      if (newMap[l.source] && newMap[l.target]) {
        edges.push({
          id: l.id,
          source: l.source,
          target: l.target,
          kind: l.kind,
          frequency: l.frequency,
          confidence: l.confidence,
          energy: l.energy
        });
      }
    }

    this.nodes = nodes;
    this.edges = edges;
    this.nodeMap = newMap;
    this._updateStats();
  };

  Graph3D.prototype._updateStats = function () {
    if (!this.statsEl) return;
    var active = 0;
    for (var i = 0; i < this.nodes.length; i++) {
      if (this.nodes[i].state === 'active') active++;
    }
    this.statsEl.textContent =
      'Conceitos: ' + this.nodes.length +
      '  Links: ' + this.edges.length +
      '  Ativos: ' + active;
  };

  // --- Physics ---

  Graph3D.prototype._simulate = function () {
    var nodes = this.nodes;
    var edges = this.edges;
    var nodeMap = this.nodeMap;
    var n = nodes.length;
    if (n === 0) return;

    // Repulsion
    for (var i = 0; i < n; i++) {
      for (var j = i + 1; j < n; j++) {
        var a = nodes[i], b = nodes[j];
        var dx = a.x - b.x, dy = a.y - b.y, dz = a.z - b.z;
        var dist2 = dx * dx + dy * dy + dz * dz + 1;
        var force = REPULSION / dist2;
        var dist = Math.sqrt(dist2);
        var fx = (dx / dist) * force;
        var fy = (dy / dist) * force;
        var fz = (dz / dist) * force;
        a.vx += fx; a.vy += fy; a.vz += fz;
        b.vx -= fx; b.vy -= fy; b.vz -= fz;
      }
    }

    // Attraction through edges
    for (var e = 0; e < edges.length; e++) {
      var edge = edges[e];
      var src = nodeMap[edge.source];
      var tgt = nodeMap[edge.target];
      if (!src || !tgt) continue;
      var dx2 = tgt.x - src.x, dy2 = tgt.y - src.y, dz2 = tgt.z - src.z;
      var fx2 = dx2 * ATTRACTION;
      var fy2 = dy2 * ATTRACTION;
      var fz2 = dz2 * ATTRACTION;
      src.vx += fx2; src.vy += fy2; src.vz += fz2;
      tgt.vx -= fx2; tgt.vy -= fy2; tgt.vz -= fz2;
    }

    // Center pull + damping + apply
    for (var k = 0; k < n; k++) {
      var nd = nodes[k];
      nd.vx -= nd.x * CENTER_PULL;
      nd.vy -= nd.y * CENTER_PULL;
      nd.vz -= nd.z * CENTER_PULL;
      nd.vx *= DAMPING;
      nd.vy *= DAMPING;
      nd.vz *= DAMPING;
      nd.x += nd.vx;
      nd.y += nd.vy;
      nd.z += nd.vz;
    }
  };

  // --- Projection ---

  Graph3D.prototype._project = function (x, y, z) {
    var cosY = Math.cos(this.rotY), sinY = Math.sin(this.rotY);
    var cosX = Math.cos(this.rotX), sinX = Math.sin(this.rotX);

    // Rotate around Y axis
    var rx = x * cosY - z * sinY;
    var rz = x * sinY + z * cosY;

    // Rotate around X axis
    var ry = y * cosX - rz * sinX;
    var rz2 = y * sinX + rz * cosX;

    var offset = 300;
    var denom = rz2 + FOV + offset;
    if (denom < 1) denom = 1;
    var scale = (FOV / denom) * this.zoom;

    return {
      sx: this.width / 2 + rx * scale,
      sy: this.height / 2 + ry * scale,
      scale: scale,
      z: rz2
    };
  };

  // --- Rendering ---

  Graph3D.prototype._render = function () {
    var ctx = this.ctx;
    var w = this.width, h = this.height;
    ctx.clearRect(0, 0, w, h);

    if (this.nodes.length === 0) {
      ctx.fillStyle = '#5c6172';
      ctx.font = '13px "DM Sans", sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText('Envie mensagens para visualizar o grafo', w / 2, h / 2);
      return;
    }

    var nodeMap = this.nodeMap;
    var selected = this.selectedNode;
    var selectedEdgeId = this.selectedEdge;

    // Project all nodes
    var projected = [];
    for (var i = 0; i < this.nodes.length; i++) {
      var nd = this.nodes[i];
      var p = this._project(nd.x, nd.y, nd.z);
      projected.push({
        node: nd,
        sx: p.sx,
        sy: p.sy,
        scale: p.scale,
        z: p.z,
        r: (4 + nd.confidence * 18) * p.scale
      });
    }

    // Build projected map for edge rendering
    var projMap = {};
    for (var pi = 0; pi < projected.length; pi++) {
      projMap[projected[pi].node.id] = projected[pi];
    }

    // Adjacency set for selected node
    var adjacentIds = null;
    if (selected) {
      adjacentIds = {};
      adjacentIds[selected] = true;
      for (var ae = 0; ae < this.edges.length; ae++) {
        var aedge = this.edges[ae];
        if (aedge.source === selected) adjacentIds[aedge.target] = true;
        if (aedge.target === selected) adjacentIds[aedge.source] = true;
      }
    }

    // Draw edges
    for (var e = 0; e < this.edges.length; e++) {
      var edge = this.edges[e];
      var sp = projMap[edge.source];
      var tp = projMap[edge.target];
      if (!sp || !tp) continue;

      var isSelectedEdge = (edge.id === selectedEdgeId);
      var edgeOpacity = edge.confidence * Math.min(sp.scale, tp.scale) * 0.7;
      if (selected) {
        if (edge.source !== selected && edge.target !== selected) {
          edgeOpacity *= 0.08;
        }
      } else if (selectedEdgeId) {
        if (!isSelectedEdge) edgeOpacity *= 0.12;
      }
      edgeOpacity = Math.max(0.02, Math.min(1, edgeOpacity));
      if (isSelectedEdge) edgeOpacity = Math.max(0.8, edgeOpacity);

      var color = LINK_COLORS[edge.kind] || '#5c6172';
      ctx.strokeStyle = hexToRgba(color, edgeOpacity);
      var baseWidth = Math.max(0.5, 1.5 * Math.min(sp.scale, tp.scale));
      ctx.lineWidth = isSelectedEdge ? baseWidth * 2.5 : baseWidth;

      if (edge.confidence < 0.2) {
        ctx.setLineDash([4, 4]);
      } else {
        ctx.setLineDash([]);
      }

      ctx.beginPath();
      ctx.moveTo(sp.sx, sp.sy);
      ctx.lineTo(tp.sx, tp.sy);
      ctx.stroke();
    }
    ctx.setLineDash([]);

    // Sort by Z for painter's algorithm (back to front)
    projected.sort(function (a, b) { return b.z - a.z; });

    // Draw nodes
    for (var ni = 0; ni < projected.length; ni++) {
      var pn = projected[ni];
      var nd2 = pn.node;
      var r = pn.r;
      if (r < 0.5) continue;

      var nodeOpacity = 0.3 + 0.7 * Math.min(1, pn.scale);
      if (selected && !adjacentIds[nd2.id]) {
        nodeOpacity *= 0.12;
      } else if (selectedEdgeId) {
        var selEdge = null;
        for (var sei = 0; sei < this.edges.length; sei++) {
          if (this.edges[sei].id === selectedEdgeId) { selEdge = this.edges[sei]; break; }
        }
        if (selEdge && nd2.id !== selEdge.source && nd2.id !== selEdge.target) {
          nodeOpacity *= 0.15;
        }
      }

      var baseColor = STATE_COLORS[nd2.state] || STATE_COLORS.archived;

      // Glow for high energy
      if (nd2.energy > 0.5 && r > 3) {
        var glow = ctx.createRadialGradient(pn.sx, pn.sy, r * 0.3, pn.sx, pn.sy, r * 2.5);
        glow.addColorStop(0, hexToRgba(baseColor, nd2.energy * 0.3 * nodeOpacity));
        glow.addColorStop(1, hexToRgba(baseColor, 0));
        ctx.fillStyle = glow;
        ctx.beginPath();
        ctx.arc(pn.sx, pn.sy, r * 2.5, 0, Math.PI * 2);
        ctx.fill();
      }

      // Node circle
      ctx.fillStyle = hexToRgba(baseColor, nodeOpacity);
      ctx.beginPath();
      ctx.arc(pn.sx, pn.sy, r, 0, Math.PI * 2);
      ctx.fill();

      // Label
      if (r > 6) {
        var fontSize = Math.max(9, Math.min(13, r * 0.9));
        ctx.font = fontSize + 'px "DM Sans", sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillStyle = hexToRgba('#e4e7f0', nodeOpacity * 0.9);
        ctx.fillText(nd2.label, pn.sx, pn.sy - r - fontSize * 0.6);
      }
    }

    // Tooltip for hovered node
    if (this.hoveredNode && projMap[this.hoveredNode]) {
      var hp = projMap[this.hoveredNode];
      var hn = hp.node;
      var lines = [
        hn.label,
        '\u27E8' + hn.frequency.toFixed(2) + ', ' + hn.confidence.toFixed(2) + '\u27E9',
        'Energia: ' + (hn.energy * 100).toFixed(0) + '%',
        'Menções: ' + hn.mentionCount
      ];

      ctx.font = '11px "DM Mono", monospace';
      var maxW = 0;
      for (var tl = 0; tl < lines.length; tl++) {
        var tw = ctx.measureText(lines[tl]).width;
        if (tw > maxW) maxW = tw;
      }

      var tipX = hp.sx + hp.r + 10;
      var tipY = hp.sy - 10;
      var padX = 8, padY = 5;
      var lineH = 15;
      var boxW = maxW + padX * 2;
      var boxH = lines.length * lineH + padY * 2;

      // Keep tooltip in canvas bounds
      if (tipX + boxW > w) tipX = hp.sx - hp.r - 10 - boxW;
      if (tipY + boxH > h) tipY = h - boxH - 4;
      if (tipY < 4) tipY = 4;

      ctx.fillStyle = 'rgba(12,13,20,0.92)';
      ctx.strokeStyle = 'rgba(42,46,62,0.8)';
      ctx.lineWidth = 1;
      roundRect(ctx, tipX, tipY, boxW, boxH, 4);
      ctx.fill();
      ctx.stroke();

      ctx.fillStyle = '#b8bcc8';
      ctx.textAlign = 'left';
      ctx.textBaseline = 'top';
      for (var tli = 0; tli < lines.length; tli++) {
        if (tli === 0) ctx.fillStyle = '#e4e7f0';
        else ctx.fillStyle = '#8b90a0';
        ctx.fillText(lines[tli], tipX + padX, tipY + padY + tli * lineH);
      }
    }
  };

  // --- Interaction ---

  Graph3D.prototype._resize = function () {
    if (!this.canvas) return;
    var rect = this.canvas.parentElement.getBoundingClientRect();
    var dpr = window.devicePixelRatio || 1;
    this.width = rect.width;
    this.height = rect.height;
    this.canvas.width = rect.width * dpr;
    this.canvas.height = rect.height * dpr;
    this.canvas.style.width = rect.width + 'px';
    this.canvas.style.height = rect.height + 'px';
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  };

  Graph3D.prototype._onMouseDown = function (e) {
    this.dragging = true;
    this.lastMouse = { x: e.clientX, y: e.clientY };
    this.canvas.style.cursor = 'grabbing';
  };

  Graph3D.prototype._onMouseMove = function (e) {
    if (this.dragging && this.lastMouse) {
      var dx = e.clientX - this.lastMouse.x;
      var dy = e.clientY - this.lastMouse.y;
      this.rotY += dx * 0.006;
      this.rotX += dy * 0.006;
      this.rotX = Math.max(-Math.PI / 2, Math.min(Math.PI / 2, this.rotX));
      this.lastMouse = { x: e.clientX, y: e.clientY };
    } else {
      this._detectHover(e);
    }
  };

  Graph3D.prototype._onMouseUp = function () {
    this.dragging = false;
    this.lastMouse = null;
    if (this.canvas) this.canvas.style.cursor = 'grab';
  };

  Graph3D.prototype._onWheel = function (e) {
    e.preventDefault();
    var delta = e.deltaY > 0 ? 0.92 : 1.08;
    this.zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, this.zoom * delta));
  };

  Graph3D.prototype._onClick = function (e) {
    var hit = this._hitTest(e);
    if (hit) {
      this.selectedNode = hit.id;
      this.selectedEdge = null;
      if (this.onSelect) this.onSelect('node', hit);
    } else {
      var edgeHit = this._hitTestEdge(e);
      if (edgeHit) {
        this.selectedEdge = edgeHit.id;
        this.selectedNode = null;
        if (this.onSelect) this.onSelect('edge', edgeHit);
      } else {
        this.selectedNode = null;
        this.selectedEdge = null;
        if (this.onSelect) this.onSelect(null, null);
      }
    }
  };

  Graph3D.prototype._onTouchStart = function (e) {
    if (e.touches.length === 1) {
      e.preventDefault();
      var t = e.touches[0];
      this.dragging = true;
      this.lastMouse = { x: t.clientX, y: t.clientY };
    }
  };

  Graph3D.prototype._onTouchMove = function (e) {
    if (e.touches.length === 1 && this.dragging && this.lastMouse) {
      e.preventDefault();
      var t = e.touches[0];
      var dx = t.clientX - this.lastMouse.x;
      var dy = t.clientY - this.lastMouse.y;
      this.rotY += dx * 0.006;
      this.rotX += dy * 0.006;
      this.rotX = Math.max(-Math.PI / 2, Math.min(Math.PI / 2, this.rotX));
      this.lastMouse = { x: t.clientX, y: t.clientY };
    } else if (e.touches.length === 2) {
      // Pinch to zoom
      e.preventDefault();
      var d = Math.hypot(
        e.touches[0].clientX - e.touches[1].clientX,
        e.touches[0].clientY - e.touches[1].clientY
      );
      if (this._lastPinchDist) {
        var ratio = d / this._lastPinchDist;
        this.zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, this.zoom * ratio));
      }
      this._lastPinchDist = d;
    }
  };

  Graph3D.prototype._detectHover = function (e) {
    var hit = this._hitTest(e);
    if (hit) {
      this.hoveredNode = hit.id;
      this.hoveredEdge = null;
      this.canvas.style.cursor = 'pointer';
    } else {
      this.hoveredNode = null;
      var edgeHit = this._hitTestEdge(e);
      this.hoveredEdge = edgeHit ? edgeHit.id : null;
      this.canvas.style.cursor = edgeHit ? 'pointer' : 'grab';
    }
  };

  Graph3D.prototype._hitTest = function (e) {
    var rect = this.canvas.getBoundingClientRect();
    var mx = e.clientX - rect.left;
    var my = e.clientY - rect.top;

    var best = null;
    var bestDist = Infinity;

    for (var i = 0; i < this.nodes.length; i++) {
      var nd = this.nodes[i];
      var p = this._project(nd.x, nd.y, nd.z);
      var r = (4 + nd.confidence * 18) * p.scale;
      var dx = mx - p.sx, dy = my - p.sy;
      var dist = Math.sqrt(dx * dx + dy * dy);
      if (dist < r + 4 && dist < bestDist) {
        bestDist = dist;
        best = nd;
      }
    }
    return best;
  };

  Graph3D.prototype._hitTestEdge = function (e) {
    var rect = this.canvas.getBoundingClientRect();
    var mx = e.clientX - rect.left;
    var my = e.clientY - rect.top;
    var best = null;
    var bestDist = 8; // max distance in px to count as hit

    for (var i = 0; i < this.edges.length; i++) {
      var edge = this.edges[i];
      var srcNode = this.nodeMap[edge.source];
      var tgtNode = this.nodeMap[edge.target];
      if (!srcNode || !tgtNode) continue;
      var sp = this._project(srcNode.x, srcNode.y, srcNode.z);
      var tp = this._project(tgtNode.x, tgtNode.y, tgtNode.z);
      var d = distPointToSegment(mx, my, sp.sx, sp.sy, tp.sx, tp.sy);
      if (d < bestDist) {
        bestDist = d;
        best = edge;
      }
    }
    return best;
  };

  // Get connected edges for a given node id
  Graph3D.prototype.getNodeEdges = function (nodeId) {
    var result = [];
    for (var i = 0; i < this.edges.length; i++) {
      var edge = this.edges[i];
      if (edge.source === nodeId || edge.target === nodeId) {
        result.push(edge);
      }
    }
    return result;
  };

  // --- Loop ---

  Graph3D.prototype._startLoop = function () {
    var self = this;
    function tick() {
      self._simulate();
      self._render();
      self.animId = requestAnimationFrame(tick);
    }
    self.animId = requestAnimationFrame(tick);
  };

  Graph3D.prototype._startPolling = function () {
    var self = this;
    this.pollTimer = setInterval(function () {
      self.refresh();
    }, POLL_INTERVAL);
  };

  // --- Helpers ---

  function hexToRgba(hex, alpha) {
    var r = parseInt(hex.slice(1, 3), 16);
    var g = parseInt(hex.slice(3, 5), 16);
    var b = parseInt(hex.slice(5, 7), 16);
    return 'rgba(' + r + ',' + g + ',' + b + ',' + alpha.toFixed(3) + ')';
  }

  function roundRect(ctx, x, y, w, h, radius) {
    ctx.beginPath();
    ctx.moveTo(x + radius, y);
    ctx.lineTo(x + w - radius, y);
    ctx.quadraticCurveTo(x + w, y, x + w, y + radius);
    ctx.lineTo(x + w, y + h - radius);
    ctx.quadraticCurveTo(x + w, y + h, x + w - radius, y + h);
    ctx.lineTo(x + radius, y + h);
    ctx.quadraticCurveTo(x, y + h, x, y + h - radius);
    ctx.lineTo(x, y + radius);
    ctx.quadraticCurveTo(x, y, x + radius, y);
    ctx.closePath();
  }

  function distPointToSegment(px, py, ax, ay, bx, by) {
    var dx = bx - ax, dy = by - ay;
    var lenSq = dx * dx + dy * dy;
    if (lenSq === 0) return Math.hypot(px - ax, py - ay);
    var t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / lenSq));
    var projX = ax + t * dx, projY = ay + t * dy;
    return Math.hypot(px - projX, py - projY);
  }

  // Expose
  window.Graph3D = Graph3D;
})();
