# 🚀 Manga Web Translator no Google Colab (GPU na Nuvem)

Este diretório contém o notebook pronto para você testar a tradução de mangás usando placas de vídeo potentes (**NVIDIA T4, L4 ou A100**) no Google Colab, sem consumir a memória ou GPU do seu computador local.

---

## 📋 Como usar passo a passo:

### 1. Abrir o Notebook no Google Colab
1. Acesse o [Google Colab](https://colab.research.google.com/).
2. Na janela que abrir, clique na aba **Upload** (Carregar).
3. Selecione o arquivo [`manga_web_translator_colab.ipynb`](file:///d:/Projects/web-translator/colab/manga_web_translator_colab.ipynb).

### 2. Ativar a GPU no Colab
1. No menu superior do Colab, clique em:  
   **Ambiente de execução** ➔ **Alterar tipo de ambiente de execução**.
2. Em **Acelerador de hardware**, escolha **T4 GPU** (gratuito) ou uma GPU do Colab Pro (A100 / L4).
3. Clique em **Salvar**.

### 3. Rodar as Células
1. No menu superior, clique em **Ambiente de execução** ➔ **Executar tudo** (ou pressione `Ctrl + F9`).
2. Aguarde cerca de 1 a 2 minutos enquanto ele instala os pacotes e inicializa a GPU.
3. Na última célula, você verá uma mensagem em destaque com a sua **URL Pública**:
   ```text
   =================================================================
   🎉 SEU SERVIDOR NA NUVEM ESTÁ ONLINE COM SUCESSO!
   👉 URL DO SERVIDOR: https://sua-url-aqui.trycloudflare.com
   =================================================================
   ```

### 4. Conectar na Extensão do Navegador
1. No seu navegador, abra o popup da extensão **Manga Web Translator**.
2. Clique no ícone de engrenagem ⚙️ (Configurações).
3. No campo **URL do servidor desktop**, apague `http://127.0.0.1:8000` e cole a URL gerada pelo Colab (ex: `https://sua-url-aqui.trycloudflare.com`).
4. Clique em **Salvar**.
5. O indicador ficará verde: **🟢 Conectado ao aplicativo desktop**.
6. Pronto! Agora qualquer página que você traduzir no navegador será processada na GPU do Google Colab!
