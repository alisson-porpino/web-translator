// Multilingual translations for Manga Web Translator desktop UI
const translations = {
  pt: {
    subtitle: "Configurações do servidor local de tradução.",
    uiLanguageLabel: "Idioma da Interface",
    serverStopped: "Servidor parado",
    serverRunning: (port) => `Servidor ativo em http://127.0.0.1:${port}`,
    serverUnavailable: "Status do servidor indisponível",
    start: "Iniciar",
    stop: "Parar",
    googleUsage: (count, cap) => cap > 0 ? `Uso diário do Google: ${count}/${cap} requisições` : `Uso diário do Google: ${count} requisições (sem limite)`,
    googleUsageUnavailable: "Uso diário do Google: indisponível",
    appRam: "RAM do App",
    sysRam: "RAM do Sistema",
    freeMemory: "Liberar Memória",
    freeing: "Liberando…",
    memoryFreed: "Memória Liberada ✓",
    freeMemoryTitle: "Descarrega os modelos da memória imediatamente",
    modelsActive: "Modelos em RAM: Ativos",
    modelsIdle: "Modelos: Desalocados (Em espera)",
    jobsProcessing: (n) => ` · ${n} tarefa(s) em processamento`,
    downloadTitle: (name) => `Baixando modelo: ${name}`,
    downloadNote: "Este download acontece apenas no primeiro uso. As próximas traduções serão locais e instantâneas.",
    extensionLegend: "Extensão do Navegador",
    extensionHint: "Já compilada e pronta para carregar — sem necessidade de compilação adicional.",
    installChrome: "Instalar no Chrome",
    installFirefox: "Instalar no Firefox",
    extensionOpening: "Abrindo…",
    folderOpened: "Pasta aberta — siga os passos abaixo.",
    stepChrome: "Uma pasta acabou de ser aberta. Acesse <code>chrome://extensions</code>, ative o <strong>Modo de desenvolvedor</strong> (canto superior direito), clique em <strong>Carregar sem compactação</strong> e selecione essa pasta.",
    stepFirefox: "Uma pasta acabou de ser aberta. Acesse <code>about:debugging#/runtime/this-firefox</code>, clique em <strong>Carregar extensão temporária…</strong> e selecione o arquivo <code>manifest.json</code> dentro dela.",
    engineLegend: "Motor de Tradução",
    enginePreference: "Preferência de motor",
    engineLocalGoogle: "Local primeiro, Google como alternativa",
    engineGoogleLocal: "Google primeiro, local como alternativa",
    engineLocalOnly: "Apenas local (gratuito, offline)",
    engineGoogleOnly: "Apenas Google",
    engineHint: "O motor local é 100% offline, gratuito e multilíngue. O Google (Gemini) também traduz qualquer idioma, mas consome a cota da sua API.",
    localModelsLegend: "Modelos do motor local",
    ocrLabel: "Modelo de OCR (Detecção e Leitura)",
    ocrPaddleDefault: "PaddleOCR-VL 1.6 (padrão)",
    inpaintingLabel: "Modelo de Inpainting (Limpeza de Balões)",
    inpaintingLamaDefault: "LaMa (padrão)",
    translationModelLabel: "Modelo de Tradução (local, offline)",
    quantizationLabel: "Quantização",
    quantizationHint: "Modelos menores/mais quantizados são mais rápidos e usam menos RAM/VRAM; modelos maiores são mais lentos, porém mais precisos. Selecionar uma quantização ainda não baixada iniciará o download automático na próxima tradução.",
    configHint: "Estas opções são salvas no arquivo de configuração do aplicativo (~/.manga-web-translator/config.toml) e passam a valer no próximo trabalho de tradução.",
    savePipelineModels: "Salvar seleção de modelos",
    saving: "Salvando…",
    pipelineModelsSaved: "Salvo com sucesso! O próximo trabalho de tradução utilizará esta configuração.",
    googleLegend: "Google (Gemini)",
    apiKeyLabel: "Chave de API (API Key)",
    apiKeyPlaceholder: "Deixe em branco se for usar apenas o motor local",
    apiKeyHint: "Obtenha sua chave em aistudio.google.com/apikey. Armazenada apenas localmente nesta máquina.",
    dailyCapLabel: "Limite diário de requisições",
    dailyCapHint: "Controlado antes de qualquer envio — independente dos alertas do Google. 0 = sem limite.",
    advancedSummary: "Avançado",
    geminiModelLabel: "Modelo do Gemini (apenas OCR + Tradução)",
    fontStackLabel: "Lista de fontes para letreiramento (caminhos .ttf/.otf, opcional, ordenado)",
    fontStackMainPlaceholder: "Caminho da fonte principal (.ttf/.otf)",
    fontStackFallbackPlaceholder: "Caminho da fonte secundária (.ttf/.otf)",
    addFont: "+ Adicionar fonte",
    fontStackHint: "Cada caractere é desenhado com a primeira fonte desta lista que contenha o glifo correspondente. Uma fonte japonesa embutida é sempre usada como reserva. Coloque sua fonte de letreiramento principal primeiro.",
    portLabel: "Porta do servidor",
    portHint: "Se o servidor estiver rodando, pare e inicie novamente após alterar para aplicar.",
    detectionThresholdLabel: "Limiar de confiança para detecção",
    detectionThresholdHint: "Confiança mínima para considerar um bloco como texto (0-1). Menor detecta mais texto, porém com mais ruído; maior é mais rigoroso. Padrão: 0.3.",
    maxThreadsLabel: "Máximo de threads de CPU para o detector local do motor Google",
    maxThreadsHint: "Limita quantos núcleos o detector local (ONNX Runtime) utiliza no caminho do Google. 0 = sem limite (usa todos os núcleos).",
    maxMemLabel: "Limite máximo de memória para este app (MB)",
    maxMemHint: "Limite de consumo de memória do aplicativo (macOS/Linux). 0 = sem limite.",
    modelsDirLabel: "Pasta de armazenamento dos modelos",
    modelsDirPlaceholder: "Padrão: manga-web-translator/models",
    modelsDirHint: "Onde os modelos de IA são baixados e armazenados. Útil para apontar para outro disco caso o disco principal esteja com pouco espaço. Exige reiniciar o aplicativo após alterar.",
    browse: "Procurar…",
    saveSettings: "Salvar configurações",
    settingsSaved: "Salvo com sucesso! As novas configurações já estão valendo.",
    willDownload: "(será baixado)",
    loading: "Carregando…",
    couldNotLoad: "Não foi possível carregar",
    saveError: (err) => `Não foi possível salvar: ${err}`,
  },
  en: {
    subtitle: "Configures the background server your browser extension talks to.",
    uiLanguageLabel: "Interface Language",
    serverStopped: "Server stopped",
    serverRunning: (port) => `Running on http://127.0.0.1:${port}`,
    serverUnavailable: "Server status unavailable",
    start: "Start",
    stop: "Stop",
    googleUsage: (count, cap) => cap > 0 ? `Google usage today: ${count}/${cap} requests` : `Google usage today: ${count} requests (no cap set)`,
    googleUsageUnavailable: "Google usage today: unavailable",
    appRam: "App RAM",
    sysRam: "System RAM",
    freeMemory: "Free Memory",
    freeing: "Freeing…",
    memoryFreed: "Memory Freed ✓",
    freeMemoryTitle: "Unload models from memory immediately",
    modelsActive: "Models in RAM: Active",
    modelsIdle: "Models: Unloaded (Idle)",
    jobsProcessing: (n) => ` · ${n} job(s) processing`,
    downloadTitle: (name) => `Downloading model: ${name}`,
    downloadNote: "This download happens on first use only. Future translations will be local and instant.",
    extensionLegend: "Browser Extension",
    extensionHint: "Already built and ready to load — no separate download or build step needed.",
    installChrome: "Install for Chrome",
    installFirefox: "Install for Firefox",
    extensionOpening: "Opening…",
    folderOpened: "Folder opened — follow the steps below.",
    stepChrome: "A folder just opened. Open <code>chrome://extensions</code>, turn on <strong>Developer mode</strong> (top right), click <strong>Load unpacked</strong> and pick that folder.",
    stepFirefox: "A folder just opened. Open <code>about:debugging#/runtime/this-firefox</code>, click <strong>Load Temporary Add-on…</strong>, and pick the <code>manifest.json</code> file inside it.",
    engineLegend: "Translation Engine",
    enginePreference: "Engine preference",
    engineLocalGoogle: "Local first, Google as fallback",
    engineGoogleLocal: "Google first, local as fallback",
    engineLocalOnly: "Local only (free, offline)",
    engineGoogleOnly: "Google only",
    engineHint: "The local engine is fully offline, free, and multilingual. Google (Gemini) also translates any language, but consumes your API quota.",
    localModelsLegend: "Local Engine Models",
    ocrLabel: "OCR model (Detection & Recognition)",
    ocrPaddleDefault: "PaddleOCR-VL 1.6 (default)",
    inpaintingLabel: "Inpainting model (Speech bubble cleaning)",
    inpaintingLamaDefault: "LaMa (default)",
    translationModelLabel: "Translation model (local, offline)",
    quantizationLabel: "Quantization",
    quantizationHint: "A smaller/more-quantized model is faster and uses less RAM/VRAM; a larger one is slower but more capable. Picking a quantization not yet downloaded triggers an automatic download on the next job.",
    configHint: "These options are saved to the application config file (~/.manga-web-translator/config.toml) and take effect on the next translation job.",
    savePipelineModels: "Save model selection",
    saving: "Saving…",
    pipelineModelsSaved: "Saved! The next translation job will use these settings.",
    googleLegend: "Google (Gemini)",
    apiKeyLabel: "API key",
    apiKeyPlaceholder: "Leave empty if you only want local",
    apiKeyHint: "Get one at aistudio.google.com/apikey. Stored only on this machine.",
    dailyCapLabel: "Daily request cap",
    dailyCapHint: "Enforced before any request is sent — independent of Google's billing alerts. 0 = unlimited.",
    advancedSummary: "Advanced",
    geminiModelLabel: "Gemini model (OCR + translation only)",
    fontStackLabel: "Lettering font stack (.ttf/.otf paths, optional, ordered)",
    fontStackMainPlaceholder: "Main lettering font .ttf/.otf path",
    fontStackFallbackPlaceholder: "Fallback font .ttf/.otf path",
    addFont: "+ Add font",
    fontStackHint: "Each character is drawn with the first font in this list that has a glyph for it. A built-in Japanese fallback is always tried last. Put your main lettering font first.",
    portLabel: "Server port",
    portHint: "If the server is running, stop and start it again (above) after changing this for it to take effect.",
    detectionThresholdLabel: "Detection confidence threshold",
    detectionThresholdHint: "How confident the text detector must be (0-1). Lower catches more but noisier regions; higher is stricter. Default: 0.3.",
    maxThreadsLabel: "Max CPU threads for Google engine detector",
    maxThreadsHint: "Caps how many threads the local text detector uses. 0 = no cap (use all cores).",
    maxMemLabel: "Max memory for this app (MB)",
    maxMemHint: "Hard cap on memory use (macOS/Linux). 0 = no cap.",
    modelsDirLabel: "Models storage directory",
    modelsDirPlaceholder: "Default: manga-web-translator/models",
    modelsDirHint: "Where AI models are downloaded and kept. Useful to point at another drive if your main drive is low on space. Requires restarting the app after changing.",
    browse: "Browse…",
    saveSettings: "Save settings",
    settingsSaved: "Saved! New settings take effect right away.",
    willDownload: "(will download)",
    loading: "Loading…",
    couldNotLoad: "Could not load",
    saveError: (err) => `Could not save: ${err}`,
  },
  es: {
    subtitle: "Configuración del servidor local de traducción.",
    uiLanguageLabel: "Idioma de la Interfaz",
    serverStopped: "Servidor detenido",
    serverRunning: (port) => `Servidor activo en http://127.0.0.1:${port}`,
    serverUnavailable: "Estado del servidor no disponible",
    start: "Iniciar",
    stop: "Detener",
    googleUsage: (count, cap) => cap > 0 ? `Uso diario de Google: ${count}/${cap} peticiones` : `Uso diario de Google: ${count} peticiones (sin límite)`,
    googleUsageUnavailable: "Uso diario de Google: no disponible",
    appRam: "RAM de la App",
    sysRam: "RAM del Sistema",
    freeMemory: "Liberar Memoria",
    freeing: "Liberando…",
    memoryFreed: "Memoria Liberada ✓",
    freeMemoryTitle: "Descarga los modelos de la memoria inmediatamente",
    modelsActive: "Modelos en RAM: Activos",
    modelsIdle: "Modelos: Descargados (En espera)",
    jobsProcessing: (n) => ` · ${n} tarea(s) en procesamiento`,
    downloadTitle: (name) => `Descargando modelo: ${name}`,
    downloadNote: "Esta descarga ocurre solo en el primer uso. Las próximas traducciones serán locales e instantáneas.",
    extensionLegend: "Extensión del Navegador",
    extensionHint: "Ya compilada y lista para cargar — no requiere descargas ni compilación adicional.",
    installChrome: "Instalar en Chrome",
    installFirefox: "Instalar en Firefox",
    extensionOpening: "Abriendo…",
    folderOpened: "Carpeta abierta — sigue los pasos a continuación.",
    stepChrome: "Se acaba de abrir una carpeta. Abre <code>chrome://extensions</code>, activa el <strong>Modo de desarrollador</strong> (arriba a la derecha), haz clic en <strong>Cargar descomprimida</strong> y selecciona esa carpeta.",
    stepFirefox: "Se acaba de abrir una carpeta. Abre <code>about:debugging#/runtime/this-firefox</code>, haz clic en <strong>Cargar complemento temporal…</strong> y selecciona el archivo <code>manifest.json</code> dentro de ella.",
    engineLegend: "Motor de Traducción",
    enginePreference: "Preferencia de motor",
    engineLocalGoogle: "Local primero, Google como alternativa",
    engineGoogleLocal: "Google primero, local como alternativa",
    engineLocalOnly: "Solo local (gratis, sin conexión)",
    engineGoogleOnly: "Solo Google",
    engineHint: "El motor local es 100% offline, gratuito y multilingüe. Google (Gemini) también traduce cualquier idioma, pero consume la cuota de tu API.",
    localModelsLegend: "Modelos del Motor Local",
    ocrLabel: "Modelo de OCR (Detección y Lectura)",
    ocrPaddleDefault: "PaddleOCR-VL 1.6 (predeterminado)",
    inpaintingLabel: "Modelo de Inpainting (Limpieza de Bocadillos)",
    inpaintingLamaDefault: "LaMa (predeterminado)",
    translationModelLabel: "Modelo de Traducción (local, sin conexión)",
    quantizationLabel: "Cuantización",
    quantizationHint: "Los modelos más pequeños/cuantizados son más rápidos y usan menos RAM/VRAM; los más grandes son más lentos pero más precisos. Seleccionar una cuantización no descargada iniciará la descarga automática en la próxima traducción.",
    configHint: "Estas opciones se guardan en el archivo de configuración (~/.manga-web-translator/config.toml) y se aplicarán en el próximo trabajo de traducción.",
    savePipelineModels: "Guardar selección de modelos",
    saving: "Guardando…",
    pipelineModelsSaved: "¡Guardado con éxito! El próximo trabajo de traducción usará esta configuración.",
    googleLegend: "Google (Gemini)",
    apiKeyLabel: "Clave de API",
    apiKeyPlaceholder: "Dejar vacío para usar solo el motor local",
    apiKeyHint: "Obtén una en aistudio.google.com/apikey. Almacenada solo en esta máquina.",
    dailyCapLabel: "Límite diario de peticiones",
    dailyCapHint: "Controlado antes de enviar cualquier petición — independiente de las alertas de Google. 0 = sin límite.",
    advancedSummary: "Avanzado",
    geminiModelLabel: "Modelo de Gemini (solo OCR + traducción)",
    fontStackLabel: "Fuentes de rotulación (rutas .ttf/.otf, opcional, ordenado)",
    fontStackMainPlaceholder: "Ruta de la fuente principal (.ttf/.otf)",
    fontStackFallbackPlaceholder: "Ruta de la fuente secundaria (.ttf/.otf)",
    addFont: "+ Añadir fuente",
    fontStackHint: "Cada carácter se dibuja con la primera fuente de esta lista que contenga el glifo correspondiente. Una fuente japonesa integrada se usa siempre como reserva.",
    portLabel: "Puerto del servidor",
    portHint: "Si el servidor está en ejecución, deténlo e inícialo nuevamente después de cambiar esto para aplicar.",
    detectionThresholdLabel: "Umbral de confianza de detección",
    detectionThresholdHint: "Confianza mínima para detectar texto (0-1). Valores menores detectan más regiones, pero con más ruido. Predeterminado: 0.3.",
    maxThreadsLabel: "Máximo de subprocesos de CPU para detector local de Google",
    maxThreadsHint: "Limita cuántos núcleos usa el detector local. 0 = sin límite (usa todos los núcleos).",
    maxMemLabel: "Límite de memoria para esta app (MB)",
    maxMemHint: "Límite de consumo de memoria de la aplicación (macOS/Linux). 0 = sin límite.",
    modelsDirLabel: "Carpeta de almacenamiento de modelos",
    modelsDirPlaceholder: "Predeterminado: manga-web-translator/models",
    modelsDirHint: "Dónde se descargan y almacenan los modelos de IA. Útil para apuntar a otro disco si el disco principal tiene poco espacio. Requiere reiniciar la aplicación después de cambiar.",
    browse: "Examinar…",
    saveSettings: "Guardar configuración",
    settingsSaved: "¡Guardado con éxito! La nueva configuración ya está activa.",
    willDownload: "(se descargará)",
    loading: "Cargando…",
    couldNotLoad: "No se pudo cargar",
    saveError: (err) => `No se pudo guardar: ${err}`,
  }
};

let currentLang = 'pt';

function getTranslation(key, ...args) {
  const dict = translations[currentLang] || translations.pt;
  const val = dict[key] || translations.en[key] || key;
  return typeof val === 'function' ? val(...args) : val;
}

function setUiLanguage(lang) {
  if (!translations[lang]) lang = 'pt';
  currentLang = lang;
  document.documentElement.lang = lang === 'pt' ? 'pt-BR' : lang;

  const sub = document.querySelector('.app-header .subtitle');
  if (sub) sub.textContent = getTranslation('subtitle');

  const uiLangLbl = document.querySelector('label[for="uiLanguage"]');
  if (uiLangLbl) uiLangLbl.textContent = getTranslation('uiLanguageLabel');

  const extLeg = document.querySelector('.extension-install legend');
  if (extLeg) extLeg.textContent = getTranslation('extensionLegend');
  const extHint = document.querySelector('.extension-install .hint');
  if (extHint) extHint.textContent = getTranslation('extensionHint');
  const chromeBtn = document.getElementById('installChromeBtn');
  if (chromeBtn) chromeBtn.textContent = getTranslation('installChrome');
  const firefoxBtn = document.getElementById('installFirefoxBtn');
  if (firefoxBtn) firefoxBtn.textContent = getTranslation('installFirefox');

  const stepCh = document.getElementById('extensionStepChrome');
  if (stepCh) stepCh.innerHTML = getTranslation('stepChrome');
  const stepFx = document.getElementById('extensionStepFirefox');
  if (stepFx) stepFx.innerHTML = getTranslation('stepFirefox');

  const engLeg = document.querySelector('#settingsForm fieldset:nth-of-type(1) legend');
  if (engLeg) engLeg.textContent = getTranslation('engineLegend');
  const engPrefLbl = document.querySelector('label[for="enginePreference"]');
  if (engPrefLbl) engPrefLbl.textContent = getTranslation('enginePreference');
  const engHint = document.querySelector('#settingsForm fieldset:nth-of-type(1) .hint');
  if (engHint) engHint.textContent = getTranslation('engineHint');

  const engPref = document.getElementById('enginePreference');
  if (engPref) {
    const opt0 = engPref.querySelector('option[value="local-then-google"]');
    if (opt0) opt0.textContent = getTranslation('engineLocalGoogle');
    const opt1 = engPref.querySelector('option[value="google-then-local"]');
    if (opt1) opt1.textContent = getTranslation('engineGoogleLocal');
    const opt2 = engPref.querySelector('option[value="local-only"]');
    if (opt2) opt2.textContent = getTranslation('engineLocalOnly');
    const opt3 = engPref.querySelector('option[value="google-only"]');
    if (opt3) opt3.textContent = getTranslation('engineGoogleOnly');
  }

  const locLeg = document.querySelector('#settingsForm fieldset:nth-of-type(2) legend');
  if (locLeg) locLeg.textContent = getTranslation('localModelsLegend');
  const ocrLbl = document.querySelector('label[for="ocrModel"]');
  if (ocrLbl) ocrLbl.textContent = getTranslation('ocrLabel');
  const inpLbl = document.querySelector('label[for="inpaintingModel"]');
  if (inpLbl) inpLbl.textContent = getTranslation('inpaintingLabel');
  const trLbl = document.querySelector('label[for="translationModel"]');
  if (trLbl) trLbl.textContent = getTranslation('translationModelLabel');
  const qLbl = document.querySelector('label[for="translationQuantization"]');
  if (qLbl) qLbl.textContent = getTranslation('quantizationLabel');
  const qHint = document.querySelector('#settingsForm fieldset:nth-of-type(2) .hint:nth-of-type(1)');
  if (qHint) qHint.textContent = getTranslation('quantizationHint');
  const cfgHint = document.querySelector('#settingsForm fieldset:nth-of-type(2) .hint:nth-of-type(2)');
  if (cfgHint) cfgHint.textContent = getTranslation('configHint');
  const savePBtn = document.getElementById('savePipelineModelsBtn');
  if (savePBtn) savePBtn.textContent = getTranslation('savePipelineModels');

  const gLeg = document.querySelector('#settingsForm fieldset:nth-of-type(3) legend');
  if (gLeg) gLeg.textContent = getTranslation('googleLegend');
  const gKeyLbl = document.querySelector('label[for="googleApiKey"]');
  if (gKeyLbl) gKeyLbl.textContent = getTranslation('apiKeyLabel');
  const gKey = document.getElementById('googleApiKey');
  if (gKey) gKey.placeholder = getTranslation('apiKeyPlaceholder');
  const gKeyHint = document.querySelector('#settingsForm fieldset:nth-of-type(3) .hint:first-of-type');
  if (gKeyHint) gKeyHint.innerHTML = getTranslation('apiKeyHint');
  const gCapLbl = document.querySelector('label[for="googleDailyCap"]');
  if (gCapLbl) gCapLbl.textContent = getTranslation('dailyCapLabel');
  const gCapHint = document.querySelector('#settingsForm fieldset:nth-of-type(3) .hint:last-of-type');
  if (gCapHint) gCapHint.textContent = getTranslation('dailyCapHint');

  const advSum = document.querySelector('details summary');
  if (advSum) advSum.textContent = getTranslation('advancedSummary');
  const gemLbl = document.querySelector('label[for="geminiModel"]');
  if (gemLbl) gemLbl.textContent = getTranslation('geminiModelLabel');
  const fontLbl = document.querySelector('details label:nth-of-type(2)');
  if (fontLbl) fontLbl.textContent = getTranslation('fontStackLabel');
  const addFBtn = document.getElementById('addFontBtn');
  if (addFBtn) addFBtn.textContent = getTranslation('addFont');
  const fontHint = document.querySelector('#fontStackList + button + .hint');
  if (fontHint) fontHint.textContent = getTranslation('fontStackHint');
  const portLbl = document.querySelector('label[for="port"]');
  if (portLbl) portLbl.textContent = getTranslation('portLabel');
  const portHint = document.querySelector('input#port + .hint');
  if (portHint) portHint.textContent = getTranslation('portHint');
  const detLbl = document.querySelector('label[for="detectionScoreThreshold"]');
  if (detLbl) detLbl.textContent = getTranslation('detectionThresholdLabel');
  const detHint = document.querySelector('input#detectionScoreThreshold + .hint');
  if (detHint) detHint.textContent = getTranslation('detectionThresholdHint');
  const thrLbl = document.querySelector('label[for="maxCpuThreads"]');
  if (thrLbl) thrLbl.textContent = getTranslation('maxThreadsLabel');
  const thrHint = document.querySelector('input#maxCpuThreads + .hint');
  if (thrHint) thrHint.textContent = getTranslation('maxThreadsHint');
  const memLbl = document.querySelector('label[for="maxMemoryMb"]');
  if (memLbl) memLbl.textContent = getTranslation('maxMemLabel');
  const memHint = document.querySelector('input#maxMemoryMb + .hint');
  if (memHint) memHint.textContent = getTranslation('maxMemHint');
  const modDirLbl = document.querySelector('label[for="modelsDir"]');
  if (modDirLbl) modDirLbl.textContent = getTranslation('modelsDirLabel');
  const modDir = document.getElementById('modelsDir');
  if (modDir) modDir.placeholder = getTranslation('modelsDirPlaceholder');
  const brwBtn = document.getElementById('browseModelsDirBtn');
  if (brwBtn) brwBtn.textContent = getTranslation('browse');
  const modDirHint = document.querySelector('details > .hint:last-of-type');
  if (modDirHint) modDirHint.textContent = getTranslation('modelsDirHint');

  const submitBtn = document.querySelector('#settingsForm button[type="submit"]');
  if (submitBtn) submitBtn.textContent = getTranslation('saveSettings');

  const unloadBtn = document.getElementById('unloadModelsBtn');
  if (unloadBtn) {
    unloadBtn.textContent = getTranslation('freeMemory');
    unloadBtn.title = getTranslation('freeMemoryTitle');
  }

  const dNote = document.querySelector('#downloadBanner p');
  if (dNote) dNote.textContent = getTranslation('downloadNote');
}

window.mwtI18n = {
  get: getTranslation,
  setLanguage: setUiLanguage,
  getCurrent: () => currentLang,
};
