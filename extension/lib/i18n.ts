export type SupportedLang = 'pt' | 'en' | 'es';

export interface I18nDict {
  settingsTitle: string;
  checkingConnection: string;
  connected: string;
  notConnected: string;
  setupHint: string;
  desktopServerUrl: string;
  targetLangLabel: string;
  uiLangLabel: string;
  getStarted: string;
  thisTab: string;
  notRegularPage: string;
  autoTranslateSite: string;
  autoTranslateHint: string;
  translatePageNow: string;
  scanningPage: string;
  scanningStarted: string;
  couldNotReachPage: (err: string) => string;
  desktopResources: string;
  connecting: string;
  appUnreachable: string;
  freeMemoryBtn: string;
  freeingMemory: string;
  memoryFreed: string;
  freeMemoryError: (err: string) => string;
  save: string;
  saved: string;
  back: string;
  settingsHint: string;
  modelsActive: string;
  modelsIdle: string;
  activeJobs: (n: number) => string;
  appRam: string;
  sysRam: string;
  stopBtn: string;
  translatedProgress: (done: number, total: number) => string;
  translatingProgress: (done: number, total: number, running: number) => string;
  selectAllTitle: string;
  selectAll: string;
  selectNone: string;
  foundImages: (n: number) => string;
  translateBtn: (n: number) => string;
  cancelBtn: string;
  rowQueued: string;
  rowTranslating: string;
  rowDone: string;
  rowError: string;
  rowCancelled: string;
  resourcesUnavailable: string;
  toastErrorTitle: string;
  toastServerError: string;
  toastTimeout: string;
  toastFallback: string;
  unsupportedImageSource: (url: string) => string;
  toggleTooltip: string;
  toggleLabel: string;
  contextMenuTitle: string;
  stoppedByUser: string;
  timedOut: string;
  translationFailed: string;
}

const DICTS: Record<SupportedLang, I18nDict> = {
  pt: {
    settingsTitle: 'Configurações',
    checkingConnection: 'Verificando conexão…',
    connected: 'Conectado ao aplicativo desktop',
    notConnected: 'Desconectado — o servidor está rodando?',
    setupHint: 'Conecte esta extensão ao aplicativo desktop. O padrão é 127.0.0.1:8000 — altere apenas se você configurou uma porta diferente no app.',
    desktopServerUrl: 'URL do servidor desktop',
    targetLangLabel: 'Idioma de destino (Mangá)',
    uiLangLabel: 'Idioma da interface',
    getStarted: 'Começar',
    thisTab: 'Esta aba',
    notRegularPage: 'Não é uma página web comum',
    autoTranslateSite: 'Traduzir automaticamente neste site',
    autoTranslateHint: 'Desativado por padrão. Ativar isso afeta apenas este site — nunca outras abas.',
    translatePageNow: 'Traduzir esta página agora',
    scanningPage: 'Escaneando página...',
    scanningStarted: 'Iniciado — acompanhe o progresso na página.',
    couldNotReachPage: (err) => `Não foi possível comunicar com a página: ${err}`,
    desktopResources: 'Recursos do app desktop',
    connecting: 'Conectando…',
    appUnreachable: 'Aplicativo desktop não alcançável.',
    freeMemoryBtn: 'Liberar Memória dos Modelos',
    freeingMemory: 'Liberando memória…',
    memoryFreed: 'Memória liberada com sucesso!',
    freeMemoryError: (err) => `Erro ao liberar: ${err}`,
    save: 'Salvar',
    saved: 'Salvo com sucesso.',
    back: '← Voltar',
    settingsHint: 'O motor de tradução e chaves de API são configurados no aplicativo desktop.',
    modelsActive: 'Modelos em RAM: Ativos',
    modelsIdle: 'Modelos: Desalocados (Em espera)',
    activeJobs: (n) => ` (${n} tarefa(s) ativa(s))`,
    appRam: 'RAM do App',
    sysRam: 'RAM do Sistema',
    stopBtn: 'Parar',
    translatedProgress: (done, total) => `Traduzido ${done}/${total}.`,
    translatingProgress: (done, total, running) =>
      `Traduzindo ${done}/${total}${running > 0 ? ` (${running} em andamento)` : ''}…`,
    selectAllTitle: 'Selecionar/desmarcar todos',
    selectAll: 'Todos',
    selectNone: 'Nenhum',
    foundImages: (n) => `${n} imagem(ns) encontrada(s)`,
    translateBtn: (n) => `Traduzir (${n})`,
    cancelBtn: 'Cancelar',
    rowQueued: 'Na fila…',
    rowTranslating: 'Traduzindo…',
    rowDone: 'Concluído',
    rowError: 'Falhou',
    rowCancelled: 'Cancelado',
    resourcesUnavailable: 'Recursos do app desktop: indisponíveis',
    toastErrorTitle: 'Erro na Tradução',
    toastServerError: 'Não foi possível conectar ao aplicativo desktop Manga Web Translator. Verifique se ele está aberto.',
    toastTimeout: 'Tempo limite atingido durante o processamento do modelo.',
    toastFallback: 'Falha ao traduzir imagem.',
    unsupportedImageSource: (url) => `Formato de imagem não suportado: ${url || '(vazio)'}`,
    toggleTooltip: 'Alternar original / tradução',
    toggleLabel: 'TR/原',
    contextMenuTitle: 'Traduzir esta imagem',
    stoppedByUser: 'Interrompido pelo usuário.',
    timedOut: 'Tempo limite esgotado aguardando o servidor.',
    translationFailed: 'Falha na tradução.',
  },
  en: {
    settingsTitle: 'Settings',
    checkingConnection: 'Checking connection…',
    connected: 'Connected to desktop app',
    notConnected: 'Not connected — is the server running?',
    setupHint: 'Connect this extension to the desktop app. It defaults to 127.0.0.1:8000 — change only if you configured a different port there.',
    desktopServerUrl: 'Desktop app server URL',
    targetLangLabel: 'Target manga language',
    uiLangLabel: 'Interface language',
    getStarted: 'Get started',
    thisTab: 'This tab',
    notRegularPage: 'Not a regular web page',
    autoTranslateSite: 'Auto-translate on this site',
    autoTranslateHint: 'Off by default everywhere. Turning this on only affects this site — never other tabs.',
    translatePageNow: 'Translate this page now',
    scanningPage: 'Scanning this page...',
    scanningStarted: 'Started — watch the page for progress.',
    couldNotReachPage: (err) => `Could not reach this page: ${err}`,
    desktopResources: 'Desktop app resources',
    connecting: 'Connecting…',
    appUnreachable: 'Desktop app not reachable.',
    freeMemoryBtn: 'Free Model Memory',
    freeingMemory: 'Freeing memory…',
    memoryFreed: 'Memory freed successfully!',
    freeMemoryError: (err) => `Error freeing memory: ${err}`,
    save: 'Save',
    saved: 'Saved.',
    back: '← Back',
    settingsHint: 'Engine and API keys are configured in the desktop app itself.',
    modelsActive: 'Models in RAM: Active',
    modelsIdle: 'Models: Unloaded (Idle)',
    activeJobs: (n) => ` (${n} active job(s))`,
    appRam: 'App RAM',
    sysRam: 'System RAM',
    stopBtn: 'Stop',
    translatedProgress: (done, total) => `Translated ${done}/${total}.`,
    translatingProgress: (done, total, running) =>
      `Translating ${done}/${total}${running > 0 ? ` (${running} running)` : ''}…`,
    selectAllTitle: 'Select/deselect all',
    selectAll: 'All',
    selectNone: 'None',
    foundImages: (n) => `${n} image(s) found`,
    translateBtn: (n) => `Translate (${n})`,
    cancelBtn: 'Cancel',
    rowQueued: 'Queued…',
    rowTranslating: 'Translating…',
    rowDone: 'Done',
    rowError: 'Failed',
    rowCancelled: 'Cancelled',
    resourcesUnavailable: 'Desktop app resources: unavailable',
    toastErrorTitle: 'Translation Error',
    toastServerError: 'Could not reach Manga Web Translator desktop app. Please make sure it is open and running.',
    toastTimeout: 'Timeout reached during model inference.',
    toastFallback: 'Failed to translate image.',
    unsupportedImageSource: (url) => `Unsupported image source: ${url || '(empty)'}`,
    toggleTooltip: 'Toggle original / translated',
    toggleLabel: 'TR/原',
    contextMenuTitle: 'Translate this image',
    stoppedByUser: 'Stopped by user.',
    timedOut: 'Timed out waiting for the backend.',
    translationFailed: 'Translation failed.',
  },
  es: {
    settingsTitle: 'Configuración',
    checkingConnection: 'Comprobando conexión…',
    connected: 'Conectado a la aplicación desktop',
    notConnected: 'Desconectado —¿el servidor está activo?',
    setupHint: 'Conecta esta extensión a la aplicación desktop. El valor predeterminado es 127.0.0.1:8000.',
    desktopServerUrl: 'URL del servidor desktop',
    targetLangLabel: 'Idioma de destino (Manga)',
    uiLangLabel: 'Idioma de la interfaz',
    getStarted: 'Comenzar',
    thisTab: 'Esta pestaña',
    notRegularPage: 'No es una página web normal',
    autoTranslateSite: 'Traducir automáticamente en este sitio',
    autoTranslateHint: 'Desactivado por defecto. Solo afecta a este sitio — nunca a otras pestañas.',
    translatePageNow: 'Traducir esta página ahora',
    scanningPage: 'Escaneando página...',
    scanningStarted: 'Iniciado — sigue el progreso en la página.',
    couldNotReachPage: (err) => `No se pudo comunicar con la página: ${err}`,
    desktopResources: 'Recursos de la app desktop',
    connecting: 'Conectando…',
    appUnreachable: 'Aplicación desktop no accesible.',
    freeMemoryBtn: 'Liberar Memoria de Modelos',
    freeingMemory: 'Liberando memoria…',
    memoryFreed: '¡Memoria liberada con éxito!',
    freeMemoryError: (err) => `Error al liberar: ${err}`,
    save: 'Guardar',
    saved: 'Guardado.',
    back: '← Volver',
    settingsHint: 'El motor y claves de API se configuran en la aplicación desktop.',
    modelsActive: 'Modelos en RAM: Activos',
    modelsIdle: 'Modelos: Descargados (En espera)',
    activeJobs: (n) => ` (${n} tarea(s) activa(s))`,
    appRam: 'RAM de la App',
    sysRam: 'RAM del Sistema',
    stopBtn: 'Detener',
    translatedProgress: (done, total) => `Traducido ${done}/${total}.`,
    translatingProgress: (done, total, running) =>
      `Traduciendo ${done}/${total}${running > 0 ? ` (${running} en curso)` : ''}…`,
    selectAllTitle: 'Seleccionar/deseleccionar todos',
    selectAll: 'Todos',
    selectNone: 'Ninguno',
    foundImages: (n) => `${n} imagen(es) encontrada(s)`,
    translateBtn: (n) => `Traducir (${n})`,
    cancelBtn: 'Cancelar',
    rowQueued: 'En cola…',
    rowTranslating: 'Traduciendo…',
    rowDone: 'Completado',
    rowError: 'Falló',
    rowCancelled: 'Cancelado',
    resourcesUnavailable: 'Recursos de la app desktop: no disponibles',
    toastErrorTitle: 'Error de Traducción',
    toastServerError: 'No se pudo conectar a Manga Web Translator. Asegúrate de que esté abierta.',
    toastTimeout: 'Tiempo límite alcanzado durante el procesamiento.',
    toastFallback: 'Error al traducir la imagen.',
    unsupportedImageSource: (url) => `Formato de imagen no soportado: ${url || '(vacío)'}`,
    toggleTooltip: 'Alternar original / traducción',
    toggleLabel: 'TR/原',
    contextMenuTitle: 'Traducir esta imagen',
    stoppedByUser: 'Interrumpido por el usuario.',
    timedOut: 'Tiempo límite agotado esperando al servidor.',
    translationFailed: 'Error en la traducción.',
  },
};

export function getI18n(lang?: string): I18nDict {
  const code = (lang?.toLowerCase().slice(0, 2) || 'pt') as SupportedLang;
  return DICTS[code] || DICTS.pt;
}
