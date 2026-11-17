/* KanGest — comportamento comum do site. A configuração (versão, downloads, contactos) vem de window.KANGEST,
 * gerada a partir de site-fonte/config.json. */
(() => {
  const C = window.KANGEST || {};
  const contacto = C.contacto || {};
  const dl = C.downloads || {};
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];

  // topo: linha ao rolar e menu no telemóvel
  const topo = $("#topo");
  const marcar = () => topo && topo.classList.toggle("rolou", scrollY > 8);
  addEventListener("scroll", marcar, { passive: true });
  marcar();
  const menu = $("#menu"), abrir = $("#abrirMenu");
  if (menu && abrir) {
    abrir.addEventListener("click", () => abrir.setAttribute("aria-expanded", menu.classList.toggle("aberto")));
    addEventListener("keydown", (e) => {
      if (e.key === "Escape" && menu.classList.contains("aberto")) { menu.classList.remove("aberto"); abrir.setAttribute("aria-expanded", "false"); abrir.focus(); }
    });
  }

  // separadores: [data-separadores] com botões [data-alvo] e painéis [data-painel] no mesmo grupo
  $$("[data-separadores]").forEach((grupo) => {
    const botoes = $$("[data-alvo]", grupo);
    const paineis = $$("[data-painel]", grupo);
    const mostrar = (alvo) => {
      botoes.forEach((b) => b.setAttribute("aria-selected", String(b.dataset.alvo === alvo)));
      paineis.forEach((p) => (p.hidden = p.dataset.painel !== alvo));
    };
    botoes.forEach((b) => b.addEventListener("click", () => mostrar(b.dataset.alvo)));
  });

  // links de download a partir da configuração
  $$("[data-download]").forEach((a) => { if (dl[a.dataset.download]) a.href = dl[a.dataset.download]; });

  // botões de contacto: só aparecem os que estão configurados
  const textoBase = (assunto) => (assunto ? `Olá! Gostaria de um orçamento: ${assunto}.` : "Olá! Gostaria de saber mais sobre os vossos sistemas.");
  const urlWhatsapp = (texto) => `https://wa.me/${contacto.whatsapp}?text=${encodeURIComponent(texto)}`;
  const urlEmail = (assunto, texto) => `mailto:${contacto.email}?subject=${encodeURIComponent(assunto || "Pedido de informação")}&body=${encodeURIComponent(texto)}`;
  const assuntoURL = new URLSearchParams(location.search).get("assunto") || "";
  $$("[data-contacto]").forEach((a) => {
    const tipo = a.dataset.contacto;
    const assunto = a.dataset.assunto || assuntoURL;
    if (tipo === "whatsapp" && contacto.whatsapp) { a.href = urlWhatsapp(textoBase(assunto)); a.hidden = false; }
    else if (tipo === "email" && contacto.email) { a.href = urlEmail(assunto, textoBase(assunto)); a.hidden = false; }
    else a.hidden = true;
  });
  $$("[data-sem-contacto]").forEach((el) => (el.hidden = !!(contacto.whatsapp || contacto.email)));
  $$("[data-com-contacto]").forEach((el) => (el.hidden = !(contacto.whatsapp || contacto.email)));

  const emailValido = (e) => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(e);
  async function enviarFormSubmit(dados) {
    const r = await fetch(`https://formsubmit.co/ajax/${contacto.email}`, {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify(dados),
    });
    if (!r.ok) throw new Error("envio");
  }

  // pedido do link de download: com email de vendas configurado, a pessoa recebe os links por email (FormSubmit)
  $$("form[data-form-download]").forEach((form) => {
    const erro = $("[data-erro]", form), botao = $("button[type=submit]", form);
    const pedido = $("[data-pedido]", form), resposta = $("[data-resposta]", form), texto = $("[data-texto-resposta]", form);
    const mostrar = (msg) => { pedido.hidden = true; texto.textContent = msg; resposta.hidden = false; resposta.querySelector("a")?.focus(); };
    form.addEventListener("submit", async (ev) => {
      ev.preventDefault();
      erro.textContent = "";
      const dados = Object.fromEntries(new FormData(form));
      if (!dados.nome?.trim()) return (erro.textContent = "Indique o seu nome.");
      if (!emailValido(dados.email || "")) return (erro.textContent = "Indique um email válido, por exemplo nome@empresa.ao.");
      const nome = dados.nome.trim().split(" ")[0];
      if (!contacto.email) return mostrar(`Obrigado, ${nome}! Pode baixar o KanGest já aqui:`);
      botao.disabled = true;
      botao.textContent = "A enviar…";
      const links = `Windows: ${dl.windows}\nMac (Apple Silicon): ${dl.macArm}\nMac (Intel): ${dl.macIntel}`;
      try {
        await enviarFormSubmit({
          ...dados,
          _subject: `Pedido de download do KanGest — ${dados.nome}`,
          _template: "table",
          _captcha: "false",
          _autoresponse: `Olá ${dados.nome}, obrigado pelo interesse no KanGest!\n\nPode baixar o instalador aqui:\n${links}\n\nNo Mac, na primeira vez abra com o botão direito → Abrir.\nQualquer dúvida, responda a este email.`,
        });
        mostrar(`Enviámos o link para ${dados.email}. Se preferir, pode baixar já aqui:`);
      } catch {
        mostrar("Não foi possível enviar o email agora, mas pode baixar já aqui:");
      }
    });
  });

  // página de entrada no KanGest web: o servidor de licenças diz o endereço da empresa e se o computador da loja está ligado
  const fe = $("#formEntrar");
  if (fe) {
    const W = C.web || {};
    const campo = $("#en-nome", fe), erro = $("[data-erro]", fe), botao = $("button[type=submit]", fe);
    const desligado = $("[data-desligado]", fe), mesmoAssim = $("[data-mesmo-assim]", fe);
    const sufixo = $("[data-sufixo]", fe);
    if (W.dominio) { sufixo.textContent = "." + W.dominio; sufixo.hidden = false; }
    const CHAVE = "kangest.empresas";
    const lerRecentes = () => { try { return JSON.parse(localStorage.getItem(CHAVE) || "[]").filter((e) => e && e.nome); } catch { return []; } };
    const gravarRecentes = (l) => { try { localStorage.setItem(CHAVE, JSON.stringify(l.slice(0, 5))); } catch { /* sem armazenamento: não lembra */ } };
    // aceita «loja», «loja.kangest.ao» ou o endereço completo colado
    const limpar = (v) => {
      let t = String(v || "").trim().toLowerCase().replace(/^https?:\/\//, "");
      t = t.split(/[/?#]/)[0];
      if (t.includes(".")) t = t.split(".")[0];
      return t;
    };
    const ir = (url, nome) => {
      if (fe.lembrar.checked) gravarRecentes([{ nome, quando: new Date().toISOString().slice(0, 10) }, ...lerRecentes().filter((e) => e.nome !== nome)]);
      location.href = url;
    };
    const mostrarRecentes = () => {
      const l = lerRecentes(), caixa = $("[data-recentes]", fe), lista = $("[data-lista-recentes]", fe);
      lista.textContent = "";
      caixa.hidden = !l.length;
      l.forEach((e) => {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "empresa";
        b.textContent = e.nome + (W.dominio ? "." + W.dominio : "");
        const q = document.createElement("small");
        q.textContent = e.quando || "";
        b.append(q);
        b.addEventListener("click", () => { campo.value = e.nome; fe.requestSubmit(); });
        lista.append(b);
      });
      if (l.length) {
        const x = document.createElement("button");
        x.type = "button";
        x.className = "esquecer";
        x.textContent = "Esquecer estas empresas neste aparelho";
        x.addEventListener("click", () => { gravarRecentes([]); mostrarRecentes(); });
        lista.append(x);
      }
    };
    const ocupado = (sim) => { botao.disabled = sim; botao.firstChild.textContent = sim ? "A procurar… " : "Continuar "; };
    fe.addEventListener("submit", async (ev) => {
      ev.preventDefault();
      erro.textContent = "";
      desligado.hidden = true;
      const nome = limpar(campo.value);
      campo.value = nome;
      if (!/^[a-z0-9-]{3,30}$/.test(nome)) return (erro.textContent = "Escreva o endereço da empresa: 3 a 30 letras minúsculas, algarismos ou hífenes.");
      if (!W.licencas) return (erro.textContent = "O acesso pela Internet ainda não está disponível.");
      ocupado(true);
      try {
        const r = await fetch(`${W.licencas}/v1/web/endereco?nome=${encodeURIComponent(nome)}`, { cache: "no-store" });
        const d = await r.json().catch(() => ({}));
        if (!r.ok || !d.ok) {
          erro.textContent = d.erro || "Não foi possível encontrar a empresa agora. Tente de novo dentro de momentos.";
        } else if (d.ligado === false) {
          mesmoAssim.href = d.endereco;
          mesmoAssim.onclick = (e) => { e.preventDefault(); ir(d.endereco, nome); };
          desligado.hidden = false;
        } else {
          return ir(d.endereco, nome);
        }
      } catch {
        erro.textContent = "Não foi possível contactar o serviço do KanGest. Verifique a ligação à Internet.";
        if (W.dominio) {
          const url = `https://${nome}.${W.dominio}`;
          mesmoAssim.href = url;
          mesmoAssim.onclick = (e) => { e.preventDefault(); ir(url, nome); };
          desligado.hidden = false;
        }
      }
      ocupado(false);
    });
    mostrarRecentes();
    const pedida = new URLSearchParams(location.search).get("empresa");
    if (pedida) { campo.value = limpar(pedida); fe.requestSubmit(); }
  }

  // pedido de contacto / orçamento
  const fc = $("#formContacto");
  if (fc) {
    const sel = $("#assunto", fc);
    if (assuntoURL && sel) {
      if (![...sel.options].some((o) => o.value === assuntoURL)) sel.add(new Option(assuntoURL, assuntoURL), 0);
      sel.value = assuntoURL;
    }
    const erro = $("[data-erro]", fc), botao = $("button[type=submit]", fc);
    fc.addEventListener("submit", async (ev) => {
      ev.preventDefault();
      erro.textContent = "";
      const d = Object.fromEntries(new FormData(fc));
      if (!d.nome?.trim()) return (erro.textContent = "Indique o seu nome.");
      if (!emailValido(d.email || "") && !d.telefone?.trim()) return (erro.textContent = "Indique um email ou um telefone para lhe respondermos.");
      const texto = `${textoBase(d.assunto)}\n\nNome: ${d.nome}\nEmpresa: ${d.empresa || "—"}\nTelefone: ${d.telefone || "—"}\nEmail: ${d.email || "—"}\n\n${d.mensagem || ""}`;
      if (contacto.email) {
        botao.disabled = true;
        botao.textContent = "A enviar…";
        try {
          await enviarFormSubmit({ ...d, _subject: `Pedido: ${d.assunto} — ${d.nome}`, _template: "table", _captcha: "false" });
          fc.hidden = true;
          $("#contactoEnviado").hidden = false;
        } catch {
          botao.disabled = false;
          botao.textContent = "Enviar pedido";
          erro.textContent = "Não foi possível enviar agora. Tente de novo dentro de momentos.";
        }
      } else if (contacto.whatsapp) {
        location.href = urlWhatsapp(texto);
      }
    });
  }
})();
