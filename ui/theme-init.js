/* Colour theme. Loaded synchronously in <head> so data-theme is on <html> before the first
   paint. The preference is "system" (follow the OS), "light" or "dark"; the page itself only
   ever sees the resolved value, data-theme="light" or "dark". The saved preference lives in
   settings.json; the copy in localStorage only exists so the very first frame is right. */
(function () {
  const KEY = "sct-theme";
  const mq = window.matchMedia ? window.matchMedia("(prefers-color-scheme: dark)") : null;
  const listeners = [];
  let pref = "system";
  const clean = (v) => (v === "light" || v === "dark" ? v : "system");
  const resolve = () => (pref === "dark" || (pref === "system" && mq && mq.matches) ? "dark" : "light");

  function paint() {
    const eff = resolve();
    const root = document.documentElement;
    const changed = root.dataset.theme !== eff;
    root.dataset.theme = eff;
    root.dataset.themePref = pref;
    return changed;
  }
  function set(next) {
    pref = clean(next);
    try { localStorage.setItem(KEY, pref); } catch (e) { /* storage unavailable: the saved setting still applies at start */ }
    if (paint()) listeners.forEach((f) => f(resolve()));
  }

  try { pref = clean(localStorage.getItem(KEY)); } catch (e) { pref = "system"; }
  paint();
  if (mq) {
    const onOs = () => { if (pref === "system" && paint()) listeners.forEach((f) => f(resolve())); };
    if (mq.addEventListener) mq.addEventListener("change", onOs); else if (mq.addListener) mq.addListener(onOs);
  }
  window.SCTTheme = { get pref() { return pref; }, get effective() { return resolve(); }, set, onChange: (f) => listeners.push(f) };
})();
