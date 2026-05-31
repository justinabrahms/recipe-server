// Recipe made-state toggles. This is deliberately browser-local; the server
// remains read-only against the Cooklang recipe directory.
(function () {
  const toggles = Array.from(document.querySelectorAll('[data-made-toggle][data-slug]'));
  if (toggles.length === 0) return;

  const keyFor = (slug) => `recipe-server:made:${slug}`;

  function readMade(slug) {
    try {
      return window.localStorage.getItem(keyFor(slug)) === '1';
    } catch (_) {
      return false;
    }
  }

  function writeMade(slug, made) {
    try {
      if (made) {
        window.localStorage.setItem(keyFor(slug), '1');
      } else {
        window.localStorage.removeItem(keyFor(slug));
      }
    } catch (_) {}
  }

  function syncSlug(slug, made) {
    toggles.forEach((toggle) => {
      if (toggle.dataset.slug === slug) toggle.checked = made;
    });
    document.querySelectorAll('li.recipe[data-slug]').forEach((li) => {
      if (li.dataset.slug === slug) li.classList.toggle('made', made);
    });
  }

  toggles.forEach((toggle) => {
    syncSlug(toggle.dataset.slug, readMade(toggle.dataset.slug));
    toggle.addEventListener('change', () => {
      const slug = toggle.dataset.slug;
      const made = toggle.checked;
      writeMade(slug, made);
      syncSlug(slug, made);
    });
  });
})();

// Recipe list page: live search filter + sticky action bar.
(function () {
  const search = document.getElementById('search');
  const list = document.getElementById('recipe-list');
  if (!list) return;

  const items = Array.from(list.querySelectorAll('li.recipe'));
  const groups = Array.from(list.querySelectorAll('.category-group'));
  if (search) {
    search.addEventListener('input', () => {
      const q = search.value.trim().toLowerCase();
      items.forEach((li) => {
        const haystack = (li.dataset.searchText || '').toLowerCase();
        if (!q || haystack.includes(q)) {
          li.classList.remove('hidden');
        } else {
          li.classList.add('hidden');
        }
      });
      groups.forEach((g) => {
        const visible = g.querySelectorAll('li.recipe:not(.hidden)').length;
        g.style.display = visible === 0 ? 'none' : '';
      });
    });
  }

  const bar = document.getElementById('action-bar');
  const counter = document.getElementById('action-count');
  function updateBar() {
    let total = 0;
    list.querySelectorAll('li.recipe').forEach((li) => {
      const cb = li.querySelector('.recipe-select');
      if (cb && cb.checked) {
        const hidden = li.querySelector('input[name^="multiplier["]');
        const n = hidden ? parseInt(hidden.value || '1', 10) : 1;
        total += Number.isFinite(n) && n > 0 ? n : 1;
      }
    });
    if (total > 0) {
      bar.classList.add('visible');
      counter.textContent =
        total === 1 ? '1 recipe' : `${total} recipes`;
    } else {
      bar.classList.remove('visible');
    }
  }

  // Toggle the per-row multiplier stepper when the checkbox flips.
  function setStepperVisibility(li) {
    const cb = li.querySelector('.recipe-select');
    const stepper = li.querySelector('.multiplier');
    if (!cb || !stepper) return;
    if (cb.checked) {
      stepper.removeAttribute('hidden');
    } else {
      stepper.setAttribute('hidden', '');
      // Reset to 1 so a re-check doesn't surprise the user with stale state.
      const out = stepper.querySelector('.step-value');
      const hidden = stepper.querySelector('input[type=hidden]');
      if (out) out.textContent = '1';
      if (hidden) {
        hidden.value = '1';
        hidden.disabled = true;
      }
    }
  }

  function nudge(li, delta) {
    const stepper = li.querySelector('.multiplier');
    const out = stepper.querySelector('.step-value');
    const hidden = stepper.querySelector('input[type=hidden]');
    let n = parseInt(out.textContent || '1', 10);
    if (!Number.isFinite(n)) n = 1;
    n = Math.max(1, Math.min(99, n + delta));
    out.textContent = String(n);
    hidden.value = String(n);
    // The hidden input is only submitted when n > 1; keeps the URL clean.
    hidden.disabled = n === 1;
  }

  list.addEventListener('change', (e) => {
    if (e.target.matches('.recipe-select')) {
      setStepperVisibility(e.target.closest('li.recipe'));
      updateBar();
    }
  });
  list.addEventListener('click', (e) => {
    const li = e.target.closest('li.recipe');
    if (!li) return;
    if (e.target.matches('.step-up')) {
      nudge(li, 1);
      updateBar();
    } else if (e.target.matches('.step-down')) {
      nudge(li, -1);
      updateBar();
    }
  });
})();

// Shopping list page: "Copy as plain text" button that drops a Notes-friendly
// blob onto the clipboard.
(function () {
  const btn = document.getElementById('copy-text-btn');
  const src = document.getElementById('copy-text-source');
  if (!btn || !src) return;
  btn.addEventListener('click', async () => {
    const text = src.value;
    try {
      await navigator.clipboard.writeText(text);
      const orig = btn.textContent;
      btn.textContent = 'Copied!';
      setTimeout(() => { btn.textContent = orig; }, 1400);
    } catch (e) {
      // Fallback: select the textarea, force-show it momentarily, exec copy.
      src.removeAttribute('hidden');
      src.select();
      try { document.execCommand('copy'); } catch (_) {}
      src.setAttribute('hidden', '');
      btn.textContent = 'Copied (fallback)';
      setTimeout(() => { btn.textContent = 'Copy as plain text'; }, 1400);
    }
  });
})();
