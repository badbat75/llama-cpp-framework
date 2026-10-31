// docs-site: interaction on top of the static charts (tools/site/src/chart.rs).
// Every chart reads the same without this file: the bars, their <title> and
// the Data table are in the HTML. This adds a tooltip that follows the
// pointer, highlights the series under it, and turns each legend entry into a
// toggle that hides or shows its series.
(function () {
  var tip = document.createElement("div");
  tip.className = "tip";
  tip.setAttribute("role", "tooltip");
  tip.hidden = true;
  document.body.appendChild(tip);

  function place(ev) {
    var pad = 14, w = tip.offsetWidth, h = tip.offsetHeight;
    var x = ev.clientX + pad, y = ev.clientY - h - pad;
    if (x + w > window.innerWidth - 8) x = ev.clientX - w - pad;
    if (y < 8) y = ev.clientY + pad;
    tip.style.left = x + "px";
    tip.style.top = y + "px";
  }

  document.querySelectorAll("figure.chart").forEach(function (fig) {
    var unit = fig.getAttribute("data-unit") || "";
    var bars = fig.querySelectorAll("g.bar");
    var off = {};

    bars.forEach(function (g) {
      // the native <title> would show on top of the tooltip
      var t = g.querySelector("title");
      if (t) t.remove();
      g.addEventListener("pointerenter", function (ev) {
        tip.innerHTML = "";
        var head = document.createElement("strong");
        head.textContent = g.dataset.s;
        var line = document.createElement("span");
        line.textContent = g.dataset.x + ": " + g.dataset.v + " " + unit;
        var n = document.createElement("small");
        n.textContent = "n = " + g.dataset.n;
        tip.append(head, line, n);
        tip.hidden = false;
        place(ev);
        fig.classList.add("focus");
        fig.querySelectorAll('[data-si="' + g.dataset.si + '"]').forEach(function (e) { e.classList.add("hot"); });
      });
      g.addEventListener("pointermove", place);
      g.addEventListener("pointerleave", function () {
        tip.hidden = true;
        fig.classList.remove("focus");
        fig.querySelectorAll(".hot").forEach(function (e) { e.classList.remove("hot"); });
      });
    });

    var legs = fig.querySelectorAll("g.leg");
    if (legs.length < 2) return;
    legs.forEach(function (leg) {
      var si = leg.dataset.si;
      leg.setAttribute("tabindex", "0");
      leg.setAttribute("role", "button");
      leg.setAttribute("aria-pressed", "true");
      function toggle() {
        var shown = Object.keys(off).filter(function (k) { return off[k]; }).length;
        if (!off[si] && shown === legs.length - 1) return; // keep one series
        off[si] = !off[si];
        leg.classList.toggle("off", off[si]);
        leg.setAttribute("aria-pressed", String(!off[si]));
        fig.querySelectorAll('g.bar[data-si="' + si + '"]').forEach(function (g) { g.classList.toggle("gone", off[si]); });
      }
      leg.addEventListener("click", toggle);
      leg.addEventListener("keydown", function (ev) {
        if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); toggle(); }
      });
    });
    var hint = document.createElement("p");
    hint.className = "hint";
    hint.textContent = "Click a legend entry to hide or show its series.";
    fig.querySelector("svg").after(hint);
  });
})();
