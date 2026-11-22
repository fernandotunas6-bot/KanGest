// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

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
  // Anti-robôs. O site não tem servidor próprio onde conferir um captcha, por isso: um campo escondido que só os
  // programas preenchem (o FormSubmit descarta o envio pelo «_honey»; aqui nem chega a sair), um mínimo de 3 segundos
  // entre abrir a página e enviar, e um envio por formulário a cada 60 segundos neste browser.
  const ABERTO = Date.now();
  const ESPERA_ENVIOS = 60_000;
  document.querySelectorAll("form.formulario").forEach((f) => {
    const mel = document.createElement("input");
    Object.assign(mel, { type: "text", name: "_honey", tabIndex: -1, autocomplete: "off", className: "mel" });
    mel.setAttribute("aria-hidden", "true");
    f.prepend(mel);
  });
  const recusa = (texto) => Object.assign(new Error(texto), { paraMostrar: texto });
  const chaveEnvio = (form) => "kg-enviado-" + (form.id || "download");

  async function enviarFormSubmit(dados, form) {
    const mel = form.querySelector("input[name=_honey]")?.value || "";
    if (mel) return; // um programa preencheu o campo escondido: parece que enviou, mas nada sai
    if (Date.now() - ABERTO < 3000) throw recusa("Espere um momento e carregue outra vez em enviar.");
    let ultimo = 0;
    try { ultimo = Number(localStorage.getItem(chaveEnvio(form))) || 0; } catch { /* sem armazenamento */ }
    if (Date.now() - ultimo < ESPERA_ENVIOS) throw recusa("Já recebemos o seu pedido há instantes. Se precisar de mudar alguma coisa, espere um minuto.");
    const r = await fetch(`https://formsubmit.co/ajax/${contacto.email}`, {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify({ ...dados, _honey: "" }),
    });
    if (!r.ok) throw new Error("envio");
    try { localStorage.setItem(chaveEnvio(form), String(Date.now())); } catch { /* sem armazenamento */ }
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
        }, form);
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
          await enviarFormSubmit({ ...d, _subject: `Pedido: ${d.assunto} — ${d.nome}`, _template: "table", _captcha: "false" }, fc);
          fc.hidden = true;
          $("#contactoEnviado").hidden = false;
        } catch (e) {
          botao.disabled = false;
          botao.textContent = "Enviar pedido";
          erro.textContent = e.paraMostrar || "Não foi possível enviar agora. Tente de novo dentro de momentos.";
        }
      } else if (contacto.whatsapp) {
        location.href = urlWhatsapp(texto);
      }
    });
  }

  // adesão: os preços vêm do servidor de licenças (painel do produtor → Planos e preços; sem preço, «sob consulta»);
  // o pedido chega por email (FormSubmit) com o email de vendas configurado, senão abre o WhatsApp já escrito
  const fa = $("#formAdesao");
  if (fa) {
    const W = C.web || {};
    const kz = (v) => `${new Intl.NumberFormat("pt-PT", { maximumFractionDigits: 0 }).format(v)} Kz`;
    if (W.licencas) {
      fetch(`${W.licencas}/v1/planos`)
        .then((r) => (r.ok ? r.json() : null))
        .then((r) => {
          for (const p of r?.planos || []) {
            const el = $(`[data-preco="${p.codigo}"]`);
            if (!el || !(p.preco_mes > 0 || p.preco_ano > 0)) continue;
            const [valor, periodo] = p.preco_mes > 0 ? [p.preco_mes, "/mês"] : [p.preco_ano, "/ano"];
            el.replaceChildren(Object.assign(document.createElement("b"), { textContent: kz(valor) }), Object.assign(document.createElement("span"), { textContent: periodo }));
            if (p.preco_mes > 0 && p.preco_ano > 0) el.append(Object.assign(document.createElement("small"), { textContent: `ou ${kz(p.preco_ano)} por ano` }));
          }
        })
        .catch(() => {});
    }
    $$("[data-com-email]").forEach((el) => (el.hidden = !contacto.email));
    $$("[data-so-whatsapp]").forEach((el) => (el.hidden = !!contacto.email || !contacto.whatsapp));
    // escolher um plano nos cartões marca-o no formulário (e ao contrário)
    const marcarCartao = (codigo) => $$("[data-plano]").forEach((c) => c.classList.toggle("escolhido", c.dataset.plano === codigo));
    const radio = (codigo) => $(`input[name=plano][data-codigo="${codigo}"]`, fa);
    $$("[data-escolher]").forEach((a) =>
      a.addEventListener("click", () => {
        const r = radio(a.dataset.escolher);
        if (r) r.checked = true;
        marcarCartao(a.dataset.escolher);
        const postos = $("#ad-postos", fa);
        if (a.dataset.escolher === "COMERCIO" && Number(postos.value) < 2) postos.value = 2;
        setTimeout(() => $("#ad-empresa", fa).focus({ preventScroll: true }), 400);
      }),
    );
    $$("input[name=plano]", fa).forEach((r) => r.addEventListener("change", () => marcarCartao(r.dataset.codigo)));
    const planoURL = (new URLSearchParams(location.search).get("plano") || "").toUpperCase();
    if (radio(planoURL)) radio(planoURL).checked = true;
    marcarCartao($("input[name=plano]:checked", fa)?.dataset.codigo);

    // NIF de Angola: 10 algarismos (empresas) ou o n.º do BI (9 algarismos, 2 letras, 3 algarismos)
    const nifValido = (n) => /^\d{10}$/.test(n) || /^\d{9}[A-Z]{2}\d{3}$/.test(n);
    const erro = $("[data-erro]", fa), botao = $("button[type=submit]", fa);
    let textoWhatsapp = "";
    fa.addEventListener("submit", async (ev) => {
      ev.preventDefault();
      erro.textContent = "";
      const f = new FormData(fa);
      const d = Object.fromEntries(f);
      const extras = f.getAll("extras");
      const nif = String(d.nif || "").replace(/[\s.-]/g, "").toUpperCase();
      const falta = (id, msg) => {
        erro.textContent = msg;
        $(id, fa)?.focus();
      };
      if (!String(d.empresa || "").trim()) return falta("#ad-empresa", "Indique o nome da empresa.");
      if (!nifValido(nif)) return falta("#ad-nif", "O NIF não parece certo: 10 algarismos (empresas) ou o n.º do BI (ex.: 001234567LA041).");
      if (!String(d.nome || "").trim()) return falta("#ad-nome", "Indique o seu nome.");
      if (String(d.telefone || "").replace(/\D/g, "").length < 9) return falta("#ad-telefone", "Indique um telefone ou WhatsApp para o contactarmos.");
      if (d.email && !emailValido(d.email)) return falta("#ad-email", "O email não parece certo.");
      if (!d.consentimento) return (erro.textContent = "Para tratarmos o pedido, autorize o contacto.");
      const linhas = [
        `Pedido de adesão ao KanGest — plano ${d.plano}`,
        "",
        `Empresa: ${d.empresa.trim()}`,
        `NIF: ${nif}`,
        `Postos de venda: ${d.postos || 1}`,
        `Actividade: ${d.ramo}`,
        `Província: ${d.provincia}`,
        `Programa actual: ${d.anterior}`,
        `Contacto: ${d.nome.trim()}${d.cargo ? ` (${d.cargo.trim()})` : ""}`,
        `Telefone: ${d.telefone.trim()}`,
        `Email: ${d.email || "—"}`,
        `Também precisa de: ${extras.length ? extras.join(", ") : "—"}`,
      ];
      if (String(d.mensagem || "").trim()) linhas.push("", d.mensagem.trim());
      const texto = linhas.join("\n");
      const enviado = (msg, comWhatsapp) => {
        fa.hidden = true;
        const caixa = $("#adesaoEnviada");
        caixa.hidden = false;
        if (msg) $("[data-mensagem-enviada]", caixa).textContent = msg;
        $("[data-reenviar]", caixa).hidden = !comWhatsapp;
        caixa.scrollIntoView({ behavior: "smooth", block: "center" });
      };
      if (contacto.email) {
        botao.disabled = true;
        botao.textContent = "A enviar…";
        try {
          await enviarFormSubmit({
            _subject: `Adesão ao KanGest: ${d.empresa.trim()} (${d.plano})`,
            _template: "table",
            _captcha: "false",
            _replyto: d.email || undefined,
            plano: d.plano,
            empresa: d.empresa.trim(),
            nif,
            postos: d.postos,
            actividade: d.ramo,
            provincia: d.provincia,
            programa_actual: d.anterior,
            nome: d.nome.trim(),
            funcao: d.cargo || "",
            telefone: d.telefone.trim(),
            email: d.email || "",
            tambem_precisa: extras.join(", "),
            mensagem: d.mensagem || "",
          }, fa);
          enviado(null, false);
        } catch (e) {
          botao.disabled = false;
          botao.textContent = "Enviar pedido de adesão";
          erro.textContent = e.paraMostrar ? e.paraMostrar : contacto.whatsapp ? "Não foi possível enviar agora. Tente de novo, ou envie pelo WhatsApp." : "Não foi possível enviar agora. Tente de novo dentro de momentos.";
        }
      } else if (contacto.whatsapp) {
        textoWhatsapp = urlWhatsapp(texto);
        enviado("O pedido abriu no WhatsApp, já escrito: carregue em enviar para nos chegar. Respondemos pelo mesmo número.", true);
        window.open(textoWhatsapp, "_blank", "noopener");
      } else {
        erro.textContent = `Ligue-nos para ${contacto.telefone || "o número indicado"}: os pedidos pelo site ainda não estão ligados.`;
      }
    });
    $("[data-abrir-whatsapp]")?.addEventListener("click", (e) => {
      e.preventDefault();
      if (textoWhatsapp) window.open(textoWhatsapp, "_blank", "noopener");
    });
  }
})();
