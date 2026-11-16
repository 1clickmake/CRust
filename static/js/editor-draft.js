(function() {
    window.initCRustDraft = function(opts) {
        var key = opts.storageKey;
        var subjectEl = document.querySelector(opts.subjectSelector || 'input[name="wr_subject"]');
        var getHtml = opts.getHtml;
        var setHtml = opts.setHtml;
        var bannerEl = document.getElementById(opts.bannerId || 'rb-draft-banner');
        var statusEl = document.getElementById(opts.statusId || 'rb-draft-status');
        var foundTpl = opts.foundMsg || 'Found autosaved draft ({time}). Restore?';
        var restoreLbl = opts.restoreLabel || 'Restore';
        var discardLbl = opts.discardLabel || 'Discard';
        var savedLbl = opts.savedLabel || 'Draft saved';

        function saveNow() {
            var subj = subjectEl ? subjectEl.value : '';
            var html = getHtml ? getHtml() : '';
            if (!subj && (!html || html === '<p><br></p>' || html === '<br>')) return;
            try {
                localStorage.setItem(key, JSON.stringify({
                    subject: subj,
                    content: html,
                    savedAt: new Date().toLocaleTimeString()
                }));
                if (statusEl) statusEl.textContent = savedLbl + ' · ' + new Date().toLocaleTimeString();
            } catch (e) {}
        }

        try {
            var raw = localStorage.getItem(key);
            if (raw && bannerEl) {
                var parsed = JSON.parse(raw);
                if (parsed && (parsed.subject || parsed.content)) {
                    bannerEl.style.display = 'flex';
                    bannerEl.innerHTML =
                        '<span><i class="fas fa-clock-rotate-left" style="color:var(--primary);"></i> '
                        + foundTpl.replace('{time}', parsed.savedAt || '')
                        + '</span>'
                        + '<span style="display:flex; gap:6px;">'
                        + '<button type="button" id="rb-draft-restore" class="btn btn-primary" style="padding:4px 10px; font-size:12px;">' + restoreLbl + '</button>'
                        + '<button type="button" id="rb-draft-discard" class="btn" style="padding:4px 10px; font-size:12px;">' + discardLbl + '</button>'
                        + '</span>';
                    document.getElementById('rb-draft-restore').onclick = function() {
                        if (subjectEl && parsed.subject) subjectEl.value = parsed.subject;
                        if (setHtml && parsed.content) setHtml(parsed.content);
                        bannerEl.style.display = 'none';
                    };
                    document.getElementById('rb-draft-discard').onclick = function() {
                        localStorage.removeItem(key);
                        bannerEl.style.display = 'none';
                    };
                }
            }
        } catch (e) {}

        var timer = setInterval(saveNow, 4000);
        if (subjectEl) subjectEl.addEventListener('input', function() {
            clearTimeout(subjectEl._draftTo);
            subjectEl._draftTo = setTimeout(saveNow, 800);
        });

        window.clearCRustDraft = function() {
            clearInterval(timer);
            try { localStorage.removeItem(key); } catch (e) {}
        };
    };
})();
