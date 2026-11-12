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
