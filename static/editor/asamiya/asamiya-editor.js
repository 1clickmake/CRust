/**
 * Asamiya Editor (Vanilla JS) + Rust Board Smart Inline Attachment Extension
 * Zero-dependency, ultra-lightweight pure Vanilla JS web editor
 */

class AsamiyaEditor {
    constructor(selector, options = {}) {
        this.container = document.querySelector(selector);
        if (!this.container) {
            console.error(`AsamiyaEditor Error: Selector '${selector}' not found.`);
            return;
        }

        this.options = {
            placeholder: options.placeholder || '여기에 내용을 입력하세요...',
            height: options.height || '300px',
        };

        this.isHTMLMode = false;
        this.initialContent = this.container.innerHTML;
        this.container.innerHTML = '';

        this.initUI();
        this.bindEvents();
    }

    initUI() {
        this.wrapper = document.createElement('div');
        this.wrapper.className = 'asamiya-editor-wrapper';

        this.toolbar = document.createElement('div');
        this.toolbar.className = 'asamiya-toolbar';
        this.toolbar.innerHTML = this.getToolbarHTML();

        this.contentArea = document.createElement('div');
        this.contentArea.className = 'asamiya-content';
        this.contentArea.contentEditable = true;
        this.contentArea.style.height = this.options.height;
        this.contentArea.setAttribute('data-placeholder', this.options.placeholder);
        this.contentArea.innerHTML = this.initialContent;

        this.htmlArea = document.createElement('textarea');
        this.htmlArea.className = 'asamiya-html-area';
        this.htmlArea.style.height = this.options.height;
        this.htmlArea.spellcheck = false;

        this.wrapper.appendChild(this.toolbar);
        this.wrapper.appendChild(this.contentArea);
        this.wrapper.appendChild(this.htmlArea);
        this.container.appendChild(this.wrapper);

        document.execCommand('defaultParagraphSeparator', false, 'p');
        document.execCommand('styleWithCSS', false, true);
    }

    getToolbarHTML() {
        return `
            <div class="asamiya-toolbar-group">
                <select class="asamiya-select" id="asamiya-fontsize" title="글자 크기">
                    <option value="">크기</option>
                    <option value="12px">12px</option>
                    <option value="14px">14px</option>
                    <option value="16px">16px</option>
                    <option value="18px">18px</option>
                    <option value="20px">20px</option>
                    <option value="24px">24px</option>
                    <option value="30px">30px</option>
                    <option value="36px">36px</option>
                </select>
            </div>
            <div class="asamiya-toolbar-group">
                <input type="color" class="asamiya-color-picker" id="asamiya-backcolor" title="배경 색상" value="#ffff00">
                <input type="color" class="asamiya-color-picker" id="asamiya-forecolor" title="글자 색상" value="#4facfe">
            </div>
            <div class="asamiya-toolbar-group">
                <button type="button" class="asamiya-btn" data-command="undo" title="실행 취소"><i class="fas fa-undo"></i></button>
                <button type="button" class="asamiya-btn" data-command="redo" title="다시 실행"><i class="fas fa-redo"></i></button>
                <button type="button" class="asamiya-btn" data-command="removeFormat" title="서식 지우기"><i class="fas fa-eraser"></i></button>
            </div>
            <div class="asamiya-toolbar-group">
                <button type="button" class="asamiya-btn" data-command="bold" title="굵게"><i class="fas fa-bold"></i></button>
                <button type="button" class="asamiya-btn" data-command="italic" title="기울임"><i class="fas fa-italic"></i></button>
                <button type="button" class="asamiya-btn" data-command="underline" title="밑줄"><i class="fas fa-underline"></i></button>
                <button type="button" class="asamiya-btn" data-command="strikeThrough" title="취소선"><i class="fas fa-strikethrough"></i></button>
            </div>
            <div class="asamiya-toolbar-group">
                <button type="button" class="asamiya-btn" data-command="formatBlock" data-value="H1" title="제목 1"><b>H1</b></button>
                <button type="button" class="asamiya-btn" data-command="formatBlock" data-value="H2" title="제목 2"><b>H2</b></button>
                <button type="button" class="asamiya-btn" data-command="formatBlock" data-value="P" title="본문"><b>P</b></button>
                <button type="button" class="asamiya-btn" data-command="formatBlock" data-value="BLOCKQUOTE" title="인용구"><i class="fas fa-quote-right"></i></button>
                <button type="button" class="asamiya-btn" data-command="insertHorizontalRule" title="구분선 삽입"><i class="fas fa-minus"></i></button>
            </div>
            <div class="asamiya-toolbar-group">
                <button type="button" class="asamiya-btn" data-command="justifyLeft" title="왼쪽 정렬"><i class="fas fa-align-left"></i></button>
                <button type="button" class="asamiya-btn" data-command="justifyCenter" title="가운데 정렬"><i class="fas fa-align-center"></i></button>
                <button type="button" class="asamiya-btn" data-command="justifyRight" title="오른쪽 정렬"><i class="fas fa-align-right"></i></button>
            </div>
            <div class="asamiya-toolbar-group">
                <button type="button" class="asamiya-btn" data-command="insertUnorderedList" title="기호 목록"><i class="fas fa-list-ul"></i></button>
                <button type="button" class="asamiya-btn" data-command="insertOrderedList" title="번호 목록"><i class="fas fa-list-ol"></i></button>
            </div>
            <div class="asamiya-toolbar-group">
                <button type="button" class="asamiya-btn" data-command="createLink" title="링크 삽입"><i class="fas fa-link"></i></button>
                <button type="button" class="asamiya-btn" data-command="insertImageBase64" title="이미지 삽입"><i class="fas fa-image"></i></button>
                <button type="button" class="asamiya-btn" data-command="insertFile" title="파일 첨부"><i class="fas fa-paperclip"></i></button>
                <button type="button" class="asamiya-btn" data-command="insertYoutube" title="유튜브 삽입"><i class="fab fa-youtube"></i></button>
                <button type="button" class="asamiya-btn" data-command="insertTable" title="표 삽입"><i class="fas fa-table"></i></button>
            </div>
            <div class="asamiya-toolbar-group">
                <button type="button" class="asamiya-btn" data-command="toggleHTML" title="HTML 편집"><i class="fas fa-code"></i></button>
            </div>
        `;
    }

    bindEvents() {
        const buttons = this.toolbar.querySelectorAll('.asamiya-btn');

        buttons.forEach(btn => {
            btn.addEventListener('click', (e) => {
                e.preventDefault();

                const command = btn.getAttribute('data-command');
                let value = btn.getAttribute('data-value') || null;

                if (command === 'createLink') {
                    value = prompt('링크 URL을 입력하세요:', 'https://');
                    if (!value) return;

                    this.contentArea.focus();
                    const selection = window.getSelection();
                    const selectedText = selection.toString() || value;
                    const linkHTML = `<a href="${this.escapeHtml(value)}" target="_blank">${this.escapeHtml(selectedText)}</a>`;
                    document.execCommand('insertHTML', false, linkHTML);
                    this.updateToolbarState(buttons);
                    return;
                }

                if (command === 'insertImageBase64') {
                    this.handleImageUpload();
                    return;
                }

                if (command === 'insertFile') {
                    this.handleFileUpload();
                    return;
                }

                if (command === 'insertYoutube') {
                    this.handleYoutubeEmbed();
                    return;
                }

                if (command === 'insertTable') {
                    this.handleTableInsert();
                    return;
                }

                if (command === 'toggleHTML') {
                    this.toggleHTMLMode(btn);
                    return;
                }

                if (this.isHTMLMode) return;

                document.execCommand(command, false, value);
                this.contentArea.focus();
                this.updateToolbarState(buttons);
            });
        });

        const fontSizeSelect = this.toolbar.querySelector('#asamiya-fontsize');
        if (fontSizeSelect) {
            fontSizeSelect.addEventListener('change', (e) => {
                if (this.isHTMLMode) return;
                const size = e.target.value;
                if (!size) return;

                this.contentArea.focus();
                document.execCommand('styleWithCSS', false, false);
                document.execCommand('fontSize', false, '7');
                document.execCommand('styleWithCSS', false, true);

                const fontElements = this.contentArea.querySelectorAll('font[size="7"]');
                fontElements.forEach(el => {
                    el.removeAttribute('size');
                    el.style.fontSize = size;
                });

                e.target.value = "";
            });
        }

        const backColorPicker = this.toolbar.querySelector('#asamiya-backcolor');
        if (backColorPicker) {
            backColorPicker.addEventListener('input', (e) => {
                if (this.isHTMLMode) return;
                this.contentArea.focus();
                document.execCommand('hiliteColor', false, e.target.value);
            });
        }

        const colorPicker = this.toolbar.querySelector('#asamiya-forecolor');
        if (colorPicker) {
            colorPicker.addEventListener('input', (e) => {
                if (this.isHTMLMode) return;
                this.contentArea.focus();
                document.execCommand('foreColor', false, e.target.value);
            });
        }

        // Drag and Drop support for images and files
        this.contentArea.addEventListener('dragover', (e) => {
            e.preventDefault();
            this.contentArea.classList.add('drag-over');
        });
        this.contentArea.addEventListener('dragleave', () => {
            this.contentArea.classList.remove('drag-over');
        });
        this.contentArea.addEventListener('drop', async (e) => {
            if (e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files.length > 0) {
                e.preventDefault();
                this.contentArea.classList.remove('drag-over');
                for (const file of e.dataTransfer.files) {
                    if (file.type.startsWith('image/')) {
                        await this.uploadImageFile(file);
                    } else {
                        await this.uploadAttachmentFile(file);
                    }
                }
            }
        });

        this.contentArea.addEventListener('keyup', () => this.updateToolbarState(buttons));
        this.contentArea.addEventListener('mouseup', () => this.updateToolbarState(buttons));
        this.contentArea.addEventListener('click', () => this.updateToolbarState(buttons));
    }

    updateToolbarState(buttons) {
        buttons.forEach(btn => {
            const command = btn.getAttribute('data-command');
            if (['bold', 'italic', 'underline', 'strikeThrough', 'justifyLeft', 'justifyCenter', 'justifyRight'].includes(command)) {
                if (document.queryCommandState(command)) {
                    btn.classList.add('active');
                } else {
                    btn.classList.remove('active');
                }
            }
        });
    }

    getUploadHeaders() {
        const headers = {};
        if (typeof asamiyaCsrfToken !== 'undefined' && asamiyaCsrfToken) {
            headers['X-CSRF-Token'] = asamiyaCsrfToken;
        }
        return headers;
    }

    handleImageUpload() {
        const input = document.createElement('input');
        input.setAttribute('type', 'file');
        input.setAttribute('accept', 'image/*');
        input.click();

        input.onchange = async () => {
            const file = input.files[0];
            if (!file) return;
            await this.uploadImageFile(file);
        };
    }

    async uploadImageFile(file) {
        if (typeof asamiyaEditorUploadUrl !== 'undefined' && asamiyaEditorUploadUrl) {
            const formData = new FormData();
            formData.append('file', file);

            try {
                const response = await fetch(asamiyaEditorUploadUrl, {
                    method: 'POST',
                    headers: this.getUploadHeaders(),
                    body: formData
                });

                const result = await response.json();
                if (result.error) {
                    alert(result.error);
                    return;
                }
                if (result.url) {
                    this.contentArea.focus();
                    document.execCommand('insertImage', false, result.url);
                    return;
                }
            } catch (error) {
                console.error('Image upload failed:', error);
                alert('이미지 업로드 중 서버 통신 오류가 발생했습니다.');
                return;
            }
        } else {
            alert('이미지 업로드 엔드포인트(asamiyaEditorUploadUrl)가 설정되지 않았습니다.');
        }
    }

    handleFileUpload() {
        const input = document.createElement('input');
        input.setAttribute('type', 'file');
        input.click();

        input.onchange = async () => {
            const file = input.files[0];
            if (!file) return;
            await this.uploadAttachmentFile(file);
        };
    }

    async uploadAttachmentFile(file) {
        if (typeof asamiyaEditorFileUploadUrl === 'undefined' || !asamiyaEditorFileUploadUrl) {
            alert('파일 업로드 URL이 설정되지 않았습니다.');
            return;
        }
        const formData = new FormData();
        formData.append('file', file);

        try {
            const response = await fetch(asamiyaEditorFileUploadUrl, {
                method: 'POST',
                headers: this.getUploadHeaders(),
                body: formData
            });

            const result = await response.json();
            if (result.error) {
                alert(result.error);
                return;
            }
            if (result.file_uuid) {
                const sizeText = this.formatBytes(result.size || 0);
                const cardHtml = `
                    <div class="asamiya-file-card" contenteditable="false" data-file-uuid="${this.escapeHtml(result.file_uuid)}">
                        <div class="file-icon"><i class="fas fa-file-arrow-down"></i></div>
                        <div class="file-info">
                            <span class="file-name">${this.escapeHtml(result.name)}</span>
                            <span class="file-size">${sizeText}</span>
                        </div>
                        <a href="${this.escapeHtml(result.download_url)}" class="btn-download" target="_blank">다운로드</a>
                    </div><p><br></p>`;
                this.contentArea.focus();
                document.execCommand('insertHTML', false, cardHtml);
            }
        } catch (error) {
            console.error('File upload failed:', error);
            alert('파일 업로드 중 서버 통신 오류가 발생했습니다.');
        }
    }

    formatBytes(bytes) {
        if (bytes === 0) return '0 B';
        const k = 1024;
        const sizes = ['B', 'KB', 'MB', 'GB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    }

    escapeHtml(str) {
        return String(str)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }

    getHTML() {
        if (this.isHTMLMode) {
            return this.htmlArea.value;
        }
        return this.contentArea.innerHTML;
    }

    setHTML(html) {
        if (this.isHTMLMode) {
            this.htmlArea.value = html;
        } else {
            this.contentArea.innerHTML = html;
        }
    }

    handleTableInsert() {
        const rows = prompt('몇 줄(행) 짜리 표를 만들까요?', '3');
        const cols = prompt('몇 칸(열) 짜리 표를 만들까요?', '3');

        const rowCount = parseInt(rows, 10);
        const colCount = parseInt(cols, 10);

        if (isNaN(rowCount) || isNaN(colCount) || rowCount <= 0 || colCount <= 0) {
            alert('올바른 숫자를 입력해 주세요.');
            return;
        }

        let tableHTML = '<table class="asamiya-table"><tbody>';
        for (let r = 0; r < rowCount; r++) {
            tableHTML += '<tr>';
            for (let c = 0; c < colCount; c++) {
                if (r === 0) {
                    tableHTML += '<th>제목</th>';
                } else {
                    tableHTML += '<td>내용</td>';
                }
            }
            tableHTML += '</tr>';
        }
        tableHTML += '</tbody></table><p><br></p>';

        this.contentArea.focus();
        document.execCommand('insertHTML', false, tableHTML);
    }

    handleYoutubeEmbed() {
        const url = prompt('유튜브 영상 URL을 입력하세요:', 'https://www.youtube.com/watch?v=...');
        if (!url) return;

        const match = url.match(/(?:youtu\.be\/|youtube\.com\/(?:embed\/|v\/|watch\?v=|watch\?.+&v=))([^&]{11})/);

        if (match && match[1]) {
            const videoId = match[1];
            const iframeHTML = `<br><iframe width="560" height="315" src="https://www.youtube.com/embed/${videoId}" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe><br>`;

            this.contentArea.focus();
            document.execCommand('insertHTML', false, iframeHTML);
        } else {
            alert('유효한 유튜브 URL이 아닙니다.');
        }
    }

    toggleHTMLMode(btn) {
        this.isHTMLMode = !this.isHTMLMode;

        if (this.isHTMLMode) {
            this.htmlArea.value = this.contentArea.innerHTML;
            this.contentArea.style.display = 'none';
            this.htmlArea.style.display = 'block';
            btn.classList.add('active');
        } else {
            this.contentArea.innerHTML = this.htmlArea.value;
            this.htmlArea.style.display = 'none';
            this.contentArea.style.display = 'block';
            btn.classList.remove('active');
        }
    }
}
