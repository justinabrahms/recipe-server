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
    const checked = list.querySelectorAll('input[type=checkbox]:checked').length;
    if (checked > 0) {
      bar.classList.add('visible');
      counter.textContent = checked === 1 ? '1 recipe' : `${checked} recipes`;
    } else {
      bar.classList.remove('visible');
    }
  }
  list.addEventListener('change', (e) => {
    if (e.target.matches('input[type=checkbox]')) updateBar();
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
