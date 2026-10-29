/* Progressive enhancement only: the page is complete without this file. */
(function () {
  "use strict";

  var LATEST = "https://github.com/pcrlnd/deew/releases/latest/download/";

  function detectPlatform() {
    var nav = navigator;
    var p =
      ((nav.userAgentData && nav.userAgentData.platform) || "") +
      " " + (nav.platform || "") + " " + (nav.userAgent || "");
    if (/android|iphone|ipad|ipod|cros/i.test(p)) return "other";
    if (/win/i.test(p)) return "windows";
    if (/linux|x11/i.test(p)) return "linux";
    return "other";
  }

  var platform = detectPlatform();

  /* Primary download button */
  var btn = document.getElementById("download-btn");
  if (btn && platform !== "other") {
    var win = platform === "windows";
    var asset = win ? "deew.exe" : "deew";
    btn.href = LATEST + asset;
    document.getElementById("download-label").textContent = win ? "Download for Windows" : "Download for Linux";
    document.getElementById("download-icon").setAttribute("href", win ? "#i-windows" : "#i-linux");
    btn.querySelector("svg").classList.add("icon-fill");
  }

  /* Tabs */
  var tabs = Array.prototype.slice.call(document.querySelectorAll('[role="tab"]'));
  function select(tab, focus) {
    tabs.forEach(function (t) {
      var on = t === tab;
      t.setAttribute("aria-selected", on ? "true" : "false");
      t.tabIndex = on ? 0 : -1;
      var panel = document.getElementById(t.getAttribute("aria-controls"));
      panel.removeAttribute("data-js-hidden");
      panel.hidden = !on;
    });
    if (focus) tab.focus();
  }
  if (tabs.length) {
    tabs.forEach(function (tab, i) {
      tab.addEventListener("click", function () { select(tab, false); });
      tab.addEventListener("keydown", function (e) {
        var n = null;
        if (e.key === "ArrowRight") n = (i + 1) % tabs.length;
        else if (e.key === "ArrowLeft") n = (i - 1 + tabs.length) % tabs.length;
        else if (e.key === "Home") n = 0;
        else if (e.key === "End") n = tabs.length - 1;
        if (n === null) return;
        e.preventDefault();
        select(tabs[n], true);
      });
    });
    select(document.getElementById(platform === "linux" ? "tab-linux" : "tab-windows"), false);
  }

  /* Copy buttons */
  function setIcon(button, name) {
    var use = button.querySelector("use");
    use.setAttribute("href", "#i-" + name);
    button.classList.toggle("copied", name === "check");
  }
  Array.prototype.forEach.call(document.querySelectorAll(".copy-btn"), function (button) {
    var label = button.getAttribute("data-label") || "command";
    var status = button.parentNode.querySelector(".copy-status");
    var timer;
    button.addEventListener("click", function () {
      var code = button.parentNode.querySelector("code").textContent;
      if (!navigator.clipboard) return;
      navigator.clipboard.writeText(code).then(function () {
        setIcon(button, "check");
        button.setAttribute("aria-label", "Copied");
        status.textContent = "Copied to clipboard";
        clearTimeout(timer);
        timer = setTimeout(function () {
          setIcon(button, "copy");
          button.setAttribute("aria-label", "Copy " + label);
          status.textContent = "";
        }, 1800);
      }, function () { /* clipboard unavailable; ignore */ });
    });
  });

  /* Dithered background. Browsers draw CSS gradients at 8 bits per channel
     without dithering, which bands on these dark glows; grain on top only hides
     the edges of the steps. This draws the same glows into canvases in floats
     and adds triangular noise before rounding, so the brightness changes
     smoothly. The CSS gradients stay as the fallback; keep the numbers below in
     sync with style.css. */
  var heroBg = document.querySelector(".hero-bg");
  var figure = document.querySelector(".hero-figure");
  if (heroBg && figure && window.ResizeObserver) {
    var BG = [8.66, 7.85, 16.92]; /* --background */
    var RING = [73, 124, 254], BRAND = [34, 63, 139];
    var VIOLET = [108.42, 55.78, 210.41], TEAL = [0, 123.42, 177.8];
    var MAX_PIXELS = 4e6;

    var STEP = 8; /* the glows are smooth: computed every 8 device px, interpolated */

    /* triangular noise in (-1, 1): a table read from a random offset per row */
    var seed = 0x9e3779b9;
    var rand = function () {
      seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5;
      return (seed >>> 0) / 4294967296;
    };
    var NOISE = new Float32Array(65536);
    for (var i = 0; i < NOISE.length; i++) NOISE[i] = rand() + rand() - 1;

    /* erf, Abramowitz and Stegun 7.1.26 */
    var erf = function (x) {
      var s = x < 0 ? -1 : 1;
      x = Math.abs(x);
      var t = 1 / (1 + 0.3275911 * x);
      return s * (1 - ((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * Math.exp(-x * x));
    };

    /* Per-axis factors of a layer on the grid, in CSS px. A radial gradient from
       its color to transparent at `end` keeps squared distances, a blurred
       rectangle (filter: blur) keeps the two erf edges. */
    var axis = function (layer, n, scale, vertical) {
      var out = new Float32Array(n);
      for (var i = 0; i < n; i++) {
        var v = (i * STEP + 0.5) / scale;
        if (layer.rect) {
          var r = layer.rect, k = 1 / (layer.blur * Math.SQRT2);
          out[i] = vertical ? 0.5 * (erf((v - r[1]) * k) - erf((v - r[3]) * k))
                            : 0.5 * (erf((v - r[0]) * k) - erf((v - r[2]) * k));
        } else {
          var d = vertical ? (v - layer.cy) / layer.ry : (v - layer.cx) / layer.rx;
          out[i] = d * d;
        }
      }
      return out;
    };

    /* Composite the layers (bottom first) over `base` (opaque) or over nothing
       on the grid, then interpolate, dither and draw them into the canvas. */
    var paint = function (canvas, layers, base) {
      var rect = canvas.getBoundingClientRect();
      var w = rect.width, h = rect.height;
      if (!w || !h) return;
      var scale = Math.min(window.devicePixelRatio || 1, Math.sqrt(MAX_PIXELS / (w * h)));
      var ow = Math.round(w * scale), oh = Math.round(h * scale);
      var gw = Math.floor((ow - 1) / STEP) + 2, gh = Math.floor((oh - 1) / STEP) + 2;
      var xs = layers.map(function (l) { return axis(l, gw, scale, false); });
      var ys = layers.map(function (l) { return axis(l, gh, scale, true); });

      /* premultiplied color and alpha per grid point */
      var grid = new Float32Array(gw * gh * 4), x, y, o;
      for (y = 0, o = 0; y < gh; y++) {
        for (x = 0; x < gw; x++, o += 4) {
          var r = 0, g = 0, b = 0, a = 0;
          if (base) { r = base[0]; g = base[1]; b = base[2]; a = 1; }
          for (var n = 0; n < layers.length; n++) {
            var l = layers[n], la;
            if (l.rect) la = l.alpha * xs[n][x] * ys[n][y];
            else la = l.alpha * Math.max(0, 1 - Math.sqrt(xs[n][x] + ys[n][y]) / l.end);
            if (la <= 0) continue;
            /* "over" in straight alpha: the layer's share of the new color */
            var na = a + la - a * la, share = la / na;
            r += (l.color[0] - r) * share;
            g += (l.color[1] - g) * share;
            b += (l.color[2] - b) * share;
            a = na;
          }
          grid[o] = r * a; grid[o + 1] = g * a; grid[o + 2] = b * a; grid[o + 3] = a;
        }
      }

      /* bilinear between grid points i0 (row above) and i1 (row below) */
      var lerp = function (i0, i1, f, fy) {
        var top = grid[i0] + (grid[i0 + 4] - grid[i0]) * f;
        return top + (grid[i1] + (grid[i1 + 4] - grid[i1]) * f - top) * fy;
      };
      var col = new Int32Array(ow), fx = new Float32Array(ow);
      for (x = 0; x < ow; x++) { col[x] = Math.floor(x / STEP) * 4; fx[x] = x / STEP - Math.floor(x / STEP); }
      var out = new ImageData(ow, oh), data = out.data;
      for (y = 0, o = 0; y < oh; y++) {
        var row = Math.floor(y / STEP), fy = y / STEP - row;
        var p0 = row * gw * 4, p1 = p0 + gw * 4;
        var off = (rand() * 65536) | 0;
        for (x = 0; x < ow; x++, o += 4) {
          var i0 = p0 + col[x], i1 = p1 + col[x], f = fx[x];
          var va = lerp(i0 + 3, i1 + 3, f, fy);
          if (va <= 0) continue;
          var k = (off + o) & 65535;
          /* straight color again; Uint8ClampedArray rounds to nearest */
          data[o] = lerp(i0, i1, f, fy) / va + NOISE[k];
          data[o + 1] = lerp(i0 + 1, i1 + 1, f, fy) / va + NOISE[(k + 1) & 65535];
          data[o + 2] = lerp(i0 + 2, i1 + 2, f, fy) / va + NOISE[(k + 2) & 65535];
          data[o + 3] = base ? 255 : va * 255 + NOISE[(k + 3) & 65535];
        }
      }
      canvas.width = ow; canvas.height = oh;
      canvas.getContext("2d").putImageData(out, 0, 0);
    };

    var rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
    var pageCanvas = document.createElement("canvas");
    var heroCanvas = document.createElement("canvas");
    pageCanvas.className = "bg-canvas";
    pageCanvas.setAttribute("aria-hidden", "true");
    document.body.insertBefore(pageCanvas, document.body.firstChild);
    heroBg.appendChild(heroCanvas);

    /* body::before: a box 20% larger than the viewport on each side */
    var paintPage = function () {
      var rect = pageCanvas.getBoundingClientRect();
      var w = rect.width * 1.4, h = rect.height * 1.4, x0 = -rect.width * 0.2, y0 = -rect.height * 0.2;
      var glow = function (color, alpha, cx, cy, rx, ry) {
        return { color: color, alpha: alpha, cx: x0 + cx * w, cy: y0 + cy * h, rx: rx * w, ry: ry * h, end: 0.7 };
      };
      paint(pageCanvas, [
        glow(BRAND, 0.45, 0.16, 0.66, 0.28, 0.30),
        glow(TEAL, 0.13, 0.62, 0.78, 0.38, 0.30),
        glow(VIOLET, 0.20, 0.80, 0.26, 0.30, 0.34),
        glow(RING, 0.26, 0.24, 0.18, 0.32, 0.28)
      ], BG);
    };
    /* .hero-bg and .hero-figure::before (inset 6%, blur(60px)) */
    var paintHero = function () {
      var hero = heroBg.getBoundingClientRect(), fig = figure.getBoundingClientRect();
      var fx = fig.left - hero.left, fy = fig.top - hero.top;
      paint(heroCanvas, [
        { color: BRAND, alpha: 0.35, cx: 0.95 * hero.width, cy: 0.3 * hero.height, rx: 40 * rem, ry: 28 * rem, end: 1 },
        { color: BRAND, alpha: 0.75, cx: 0.2 * hero.width, cy: -0.1 * hero.height, rx: 60 * rem, ry: 30 * rem, end: 1 },
        { color: RING, alpha: 0.35, blur: 60, rect: [
          fx + 0.06 * fig.width, fy + 0.06 * fig.height, fx + 0.94 * fig.width, fy + 0.94 * fig.height
        ] }
      ], null);
    };

    var sizes = { page: "", hero: "" };
    var update = function () {
      var page = pageCanvas.getBoundingClientRect(), hero = heroBg.getBoundingClientRect();
      var fig = figure.getBoundingClientRect();
      var pageSize = page.width + "x" + page.height;
      var heroSize = [hero.width, hero.height, fig.left - hero.left, fig.top - hero.top, fig.width, fig.height].join();
      try {
        if (pageSize !== sizes.page) { paintPage(); sizes.page = pageSize; }
        if (heroSize !== sizes.hero) { paintHero(); sizes.hero = heroSize; }
        document.documentElement.classList.add("dithered");
      } catch (e) { /* out of memory: keep the CSS gradients */ }
    };
    var timer;
    var later = function () { clearTimeout(timer); timer = setTimeout(update, 150); };
    window.addEventListener("resize", later);
    new ResizeObserver(later).observe(heroBg);
    update();
  }
})();
