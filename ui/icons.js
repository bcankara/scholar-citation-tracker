/* Icon fallback for systems without the Windows icon font.

   The interface writes its icons as private-use code points of "Segoe Fluent Icons"
   (<span class="icon">&#xE80F;</span>). That font ships with Windows and is installed
   nowhere else, so on macOS every icon comes out as a missing glyph. Where the font is
   there this file does nothing at all and Windows keeps drawing the system font, glyph
   for glyph; where it is missing each code point below is swapped for the same shape as
   inline SVG, which inherits the colour and the size of the element it replaces, so the
   existing .icon / .sicon / .empty-icon / .sort rules keep working untouched.

   A code point with no entry here is left alone and named once in the console, so an
   icon added upstream shows up as a missing shape rather than a silent blank. */

(function () {
  "use strict";

  /* Drawn on a 16 x 16 grid. The comment on each line is the control it belongs to, not
     the Microsoft icon name, because that is what decides whether the shape is right. */
  var PATHS = {
    E192: '<circle cx="5.4" cy="10.6" r="2.6"/><path d="M7.3 8.7 13.4 2.6"/><path d="M10.8 5.2l1.5 1.5M12.3 3.7l1.5 1.5"/>', // API key
    E70D: '<path d="M3.4 6.2 8 10.8l4.6-4.6"/>', // sort descending
    E70E: '<path d="M3.4 9.8 8 5.2l4.6 4.6"/>', // sort ascending
    E70F: '<path d="M11.2 2.6 13.4 4.8 5.6 12.6l-3 .8.8-3z"/><path d="M9.8 4l2.2 2.2"/>', // correct the DOI
    E711: '<path d="M3.8 3.8l8.4 8.4M12.2 3.8l-8.4 8.4"/>', // close, dismiss, error
    E713: '<circle cx="8" cy="8" r="4.5"/><circle cx="8" cy="8" r="1.9"/><path d="M8 3.5V1.7M8 12.5v1.8M12.5 8h1.8M3.5 8H1.7M11.2 4.8l1.3-1.3M4.8 11.2l-1.3 1.3M11.2 11.2l1.3 1.3M4.8 4.8 3.5 3.5"/>', // Settings
    E715: '<rect x="1.9" y="3.5" width="12.2" height="9" rx="1.1"/><path d="M2.5 4.7 8 8.8l5.5-4.1"/>', // mail the OpenAlex team
    E716: '<circle cx="6" cy="5" r="2.1"/><path d="M2.4 13.2c0-2 1.6-3.5 3.6-3.5s3.6 1.5 3.6 3.5"/><circle cx="11.6" cy="5.9" r="1.5"/><path d="M11 10c1.7-.2 3 1 3 2.8"/>', // citing works, ORCID
    E71A: '<rect x="3.4" y="3.4" width="9.2" height="9.2" rx="1.4"/>', // stop the scan
    E71B: '<path d="M6.6 9.4a2.8 2.8 0 0 0 4 0l2-2a2.8 2.8 0 0 0-4-4l-.9.9"/><path d="M9.4 6.6a2.8 2.8 0 0 0-4 0l-2 2a2.8 2.8 0 0 0 4 4l.9-.9"/>', // DOI link
    E721: '<circle cx="7" cy="7" r="4.2"/><path d="M10.1 10.1 13.4 13.4"/>', // search box
    E72B: '<path d="M13.4 8H2.6"/><path d="M7 3.6 2.6 8 7 12.4"/>', // back to the Credits list
    E72C: '<path d="M13.4 8A5.4 5.4 0 1 1 11.8 4.2"/><path d="M13.6 2.2v3.4h-3.4"/>', // Refresh now, check for updates
    E72D: '<path d="M9.2 13.6H4c-.5 0-.9-.4-.9-.9V3.3c0-.5.4-.9.9-.9h4.3l3 3v2"/><path d="M8.2 2.4v3.1h3.1"/><path d="M8.6 11.2h4.8"/><path d="M11.6 9.4l1.8 1.8-1.8 1.8"/>', // export CSV / Excel
    E73E: '<path d="M3.2 8.4 6.4 11.6 12.8 4.6"/>', // scan step done
    E74D: '<path d="M2.8 4.4h10.4"/><path d="M6.4 4.4V2.9c0-.3.2-.5.5-.5h2.2c.3 0 .5.2.5.5v1.5"/><path d="M4.2 4.4v8.4c0 .5.4.9.9.9h5.8c.5 0 .9-.4.9-.9V4.4"/><path d="M6.8 7v4.4M9.2 7v4.4"/>', // delete history, uninstall
    E74E: '<path d="M2.6 3.4c0-.4.4-.8.8-.8h7.4l2.6 2.6v7.4c0 .4-.4.8-.8.8H3.4c-.4 0-.8-.4-.8-.8z"/><path d="M5.2 2.6v3.6h5.2V2.6"/><path d="M5.2 13.4V9.6h5.6v3.8"/>', // Save
    E768: '<path fill="currentColor" stroke="none" d="M5.2 3.1 12.3 8l-7.1 4.9z"/>', // Start, Run now
    E76B: '<path d="M9.8 3.4 5.2 8l4.6 4.6"/>', // previous month / year
    E76C: '<path d="M6.2 3.4 10.8 8l-4.6 4.6"/>', // next month / year
    E774: '<circle cx="8" cy="8" r="5.8"/><path d="M2.2 8h11.6"/><path d="M8 2.2c1.6 1.7 2.4 3.6 2.4 5.8S9.6 12.1 8 13.8C6.4 12.1 5.6 10.2 5.6 8S6.4 3.9 8 2.2z"/>', // language
    E777: '<path d="M2.6 8a5.4 5.4 0 1 0 1.6-3.8"/><path d="M2.4 2.2v3.4h3.4"/>', // undo the manual match
    E77B: '<circle cx="8" cy="5.4" r="2.6"/><path d="M2.8 13.6c0-2.6 2.3-4.2 5.2-4.2s5.2 1.6 5.2 4.2"/>', // Scholar profile
    E77F: '<path d="M5.4 3.2H4c-.5 0-.9.4-.9.9v8.6c0 .5.4.9.9.9h8c.5 0 .9-.4.9-.9V4.1c0-.5-.4-.9-.9-.9h-1.4"/><rect x="5.4" y="1.8" width="5.2" height="2.6" rx=".7"/>', // paste a DOI
    E783: '<circle cx="8" cy="8" r="5.8"/><path d="M8 4.6v4"/><circle cx="8" cy="11.1" r=".75" fill="currentColor" stroke="none"/>', // update check failed
    E787: '<rect x="2.2" y="3.2" width="11.6" height="10.6" rx="1.2"/><path d="M2.2 6.4h11.6M5.4 1.8v2.6M10.6 1.8v2.6"/>', // report range, weekly summary
    E790: '<path d="M8 2.2c3.2 0 5.8 2.3 5.8 5.2 0 1.6-1.3 2.6-2.9 2.6h-.9c-.8 0-1.4.6-1.4 1.3 0 .3.1.6.3.9.2.2.3.5.3.8 0 .6-.5 1.1-1.2 1.1-3.2 0-5.8-2.7-5.8-5.9S4.8 2.2 8 2.2z"/><circle cx="5.6" cy="7.2" r=".8" fill="currentColor" stroke="none"/><circle cx="8" cy="5.4" r=".8" fill="currentColor" stroke="none"/><circle cx="10.6" cy="7.2" r=".8" fill="currentColor" stroke="none"/>', // icon set credit
    E7BA: '<path d="M8 2.4 1.9 13.2h12.2z"/><path d="M8 6.3v3.3"/><circle cx="8" cy="11.5" r=".75" fill="currentColor" stroke="none"/>', // warning, unmatched article
    E7E8: '<path d="M8 2.2v6"/><path d="M11.8 4.6a5.4 5.4 0 1 1-7.6 0"/>', // run at login
    E7F4: '<rect x="1.8" y="2.8" width="12.4" height="8.2" rx="1.2"/><path d="M5.5 13.6h5M8 11v2.6"/>', // window background
    E80F: '<path d="M2.2 7.4 8 2.4l5.8 5"/><path d="M3.8 6.7v6.1c0 .4.3.7.7.7h7c.4 0 .7-.3.7-.7V6.7"/><path d="M6.5 13.5V9.5h3v4"/>', // Dashboard
    E81C: '<path d="M2.4 4h7M2.4 7.2h4.4M2.4 10.4h3.4"/><circle cx="11" cy="10.4" r="3.4"/><path d="M11 8.7v1.9l1.4.8"/>', // Fetch log, historical data
    E895: '<path d="M3 7.4a5 5 0 0 1 8.4-2.6l1.6 1.6"/><path d="M13.2 2.6v3.8H9.4"/><path d="M13 8.6a5 5 0 0 1-8.4 2.6L3 9.6"/><path d="M2.8 13.4V9.6h3.8"/>', // update available, auto-update
    E896: '<path d="M8 2.4v7.3"/><path d="M5.2 7 8 9.8 10.8 7"/><path d="M2.8 11.8v1c0 .4.4.8.8.8h8.8c.4 0 .8-.4.8-.8v-1"/>', // install the update
    E897: '<circle cx="8" cy="8" r="5.8"/><path d="M6.3 6.2a1.75 1.75 0 0 1 3.4.3c0 1.2-1.7 1.4-1.7 2.7"/><circle cx="8" cy="11.2" r=".75" fill="currentColor" stroke="none"/>', // Help
    E8A5: '<path d="M4 2h4.4l3.6 3.6V14H4z"/><path d="M8.4 2v3.6H12"/>', // Articles, create the report
    E8A7: '<path d="M9.4 2.4h4.2v4.2"/><path d="M13.6 2.4 7.6 8.4"/><path d="M12 9.6v3.2c0 .5-.4.9-.9.9H3.3c-.5 0-.9-.4-.9-.9V5c0-.5.4-.9.9-.9h3.2"/>', // open the folder / page / Scholar
    E8B7: '<path d="M1.9 4.4c0-.5.4-.9.9-.9h3l1.5 1.8h6c.5 0 .9.4.9.9v6.3c0 .5-.4.9-.9.9H2.8c-.5 0-.9-.4-.9-.9z"/>', // data folder
    E8D2: '<path d="M3.4 12.8 7.3 3.4h1.4l3.9 9.4"/><path d="M5 9.6h6"/>', // typeface credit
    E916: '<circle cx="8" cy="8" r="5.8"/><path d="M8 4.5v3.7l2.4 1.4"/>', // check interval, report presets
    E946: '<circle cx="8" cy="8" r="5.8"/><path d="M8 7.4v3.6"/><circle cx="8" cy="5.2" r=".75" fill="currentColor" stroke="none"/>', // information
    E9D2: '<path d="M2.4 2.2v11.4h11.2"/><path d="M4.6 10.8 7.3 7.6l2.3 2 3.3-4.3"/>', // Analysis
    E9D9: '<path d="M2.4 13.6h11.2"/><path d="M4.5 13.6V8.2M8 13.6V4.4M11.5 13.6v-3.4"/>', // charts, no data yet
    E9F9: '<path d="M4 2h4.4l3.6 3.6V14H4z"/><path d="M8.4 2v3.6H12"/><path d="M6.2 11.6V9.4M8 11.6V7.6M9.8 11.6v-1.6"/>', // Report
    EA8F: '<path d="M4.2 11.2V7.4a3.8 3.8 0 0 1 7.6 0v3.8l1.2 1.4H3z"/><path d="M6.4 12.6a1.7 1.7 0 0 0 3.2 0"/>', // notifications
  };

  /* True only when the icon font can actually draw.

     Both families ship with Windows and with nothing else, so anywhere else the answer is
     no without measuring; that keeps the Windows build provably untouched, whatever the
     measurement would have said. On Windows the probe glyph is measured against a family
     that is certainly there: an unavailable family falls back to it and the two widths
     come out equal, so an install without either font still gets the shapes below. */
  function hasIconFont() {
    if (!/Windows/i.test(navigator.userAgent)) return false;
    try {
      var c = document.createElement("canvas").getContext("2d");
      if (!c) return true; // on Windows, assume the system font rather than redraw it
      var probe = "";
      c.font = "32px monospace";
      var plain = c.measureText(probe).width;
      c.font = '32px "Segoe Fluent Icons", "Segoe MDL2 Assets", monospace';
      return c.measureText(probe).width !== plain;
    } catch (e) {
      return true;
    }
  }

  if (hasIconFont()) return;
  document.documentElement.classList.add("no-fluent-icons");

  var missing = {};

  function convert(el) {
    /* Only an element that holds the code point and nothing else. A button around an icon
       reads as that one character too while the label beside it is still empty, which it is
       until the first language is applied, so without this the whole button is overwritten
       and the label and the selection pill it also holds never come back. */
    if (el.childNodes.length !== 1 || el.firstChild.nodeType !== 3) return;
    var text = el.textContent;
    if (text.length !== 1) return; // already an <svg>, or not an icon element
    var code = text.codePointAt(0);
    if (code < 0xe000 || code > 0xf8ff) return;
    var key = code.toString(16).toUpperCase();
    var shape = PATHS[key];
    if (!shape) {
      if (!missing[key]) {
        missing[key] = true;
        console.warn("icons.js: no shape for U+" + key);
      }
      return;
    }
    el.innerHTML =
      '<svg class="svg-icon" viewBox="0 0 16 16" width="1em" height="1em" fill="none" ' +
      'stroke="currentColor" stroke-width="1.3" stroke-linecap="round" ' +
      'stroke-linejoin="round" aria-hidden="true" focusable="false">' + shape + "</svg>";
  }

  function paint(node) {
    if (!node || node.nodeType !== 1) return;
    convert(node);
    var kids = node.querySelectorAll("*");
    for (var i = 0; i < kids.length; i++) convert(kids[i]);
  }

  /* The tables, the scan panel and the article rows are rendered again and again, and
     app.js also writes a bare code point back into an existing span (the Start button).
     Both arrive as childList records, so the target is repainted as well as what was
     added. Replacing the glyph empties textContent, so the repaint of our own change
     stops at the length check above. */
  function start() {
    paint(document.body);
    new MutationObserver(function (records) {
      for (var i = 0; i < records.length; i++) {
        paint(records[i].target);
        var added = records[i].addedNodes;
        for (var j = 0; j < added.length; j++) paint(added[j]);
      }
    }).observe(document.body, { childList: true, subtree: true });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
