import { Fragment, type ReactNode } from "react";
import { createPortal } from "react-dom";
import type { DocCompleto } from "@/lib/types";
import { useTalao58 } from "@/lib/hooks";
import { data, dataHora, extenso, kzNum, pct, qtd } from "@/lib/format";
import { qrcode, qrSvgPath, urlFacturaAgt } from "@/lib/qrcode";

function rodapeAgt(d: DocCompleto) {
  const ctrl = d.documento.hash_controlo;
  if (!ctrl) return null;
  // documentos importados foram processados pelo programa anterior (KIGEST)
  const cert = (d.documento.importado ? d.empresa.certificado_anterior : d.empresa.certificado)?.trim();
  if (d.documento.importado) return `${ctrl}-Processado por programa válido n.º ${cert || "(KIGEST)"}`;
  return `${ctrl}-Processado por programa válido n.º ${cert || "(por certificar)"}/AGT`;
}

const MOTIVO_TXT: Record<string, string> = {
  M00: "Regime Transitório",
  M02: "Transmissão de bens e serviço não sujeita",
  M04: "IVA – Regime de Não Sujeição",
};

/**
 * Nota de entrega (NotaEntregaTk do KIGEST POS): artigos e quantidades sem preços, para acompanhar a mercadoria,
 * com as assinaturas de quem entregou e de quem recebeu. Não é um documento fiscal.
 */
export function NotaEntrega({ d, talao }: { d: DocCompleto; talao: boolean }) {
  const doc = d.documento;
  const e = d.empresa;
  const t58 = useTalao58();
  const consumidor = doc.entidade_nif === "999999999";
  if (talao) {
    return (
      <div className={`print-ticket mx-auto bg-white font-mono text-black ${t58 ? "talao-58 w-[220px] p-2 text-[10px] leading-[1.3]" : "w-[300px] p-3 text-[11px] leading-[1.35]"}`}>
        <div className="text-center">
          <div className="text-[13px] font-bold">{e.designacao_comercial || e.nome}</div>
          <div>NIF: {e.nif}</div>
          {e.endereco && <div>{e.endereco}</div>}
          {e.telefone && <div>Tel: {e.telefone}</div>}
        </div>
        <div className="my-2 border-t border-dashed border-black" />
        <div className="text-center font-bold">NOTA DE ENTREGA</div>
        <div className="text-center">
          Ref. {doc.tipo_desc} {doc.numero_doc}
        </div>
        <div className="mt-1">Data: {dataHora(doc.data_hora)}</div>
        <div>Cliente: {doc.entidade_nome}</div>
        {!consumidor && <div>NIF: {doc.entidade_nif}</div>}
        <div className="my-2 border-t border-dashed border-black" />
        <div className="flex justify-between font-bold">
          <span>Descrição</span>
          <span>Qtd</span>
        </div>
        {d.linhas.map((l) => (
          <div key={l.id} className="flex justify-between gap-2">
            <span>{l.descricao}</span>
            <span className="shrink-0">
              {qtd(l.quantidade)} {l.unidade ?? ""}
            </span>
          </div>
        ))}
        <div className="my-2 border-t border-dashed border-black" />
        <div className="mt-6 border-t border-black pt-0.5 text-center">Entregou</div>
        <div className="mt-6 border-t border-black pt-0.5 text-center">Recebeu</div>
        <div className="mt-3 text-center">Este documento não serve de factura.</div>
        <div className="mt-1 text-center">* OBRIGADO, VOLTE SEMPRE *</div>
      </div>
    );
  }
  return (
    <div className="mx-auto w-[190mm] bg-white p-2 text-[11.5px] leading-snug text-black">
      <div className="flex items-start justify-between gap-6">
        <div>
          {e.logo && <img src={e.logo} alt="" className="mb-1 max-h-16 max-w-[45mm] object-contain" />}
          <div className="text-[15px] font-bold">{e.designacao_comercial || e.nome}</div>
          <div>NIF: {e.nif}</div>
          {e.endereco && <div>{e.endereco}</div>}
          {e.telefone && <div>Tel: {e.telefone}</div>}
        </div>
        <div className="text-right">
          <div className="text-[16px] font-bold">NOTA DE ENTREGA</div>
          <div>
            Ref. {doc.tipo_desc} {doc.numero_doc}
          </div>
          <div>Data: {data(doc.data_emissao)}</div>
        </div>
      </div>
      <div className="my-4 rounded border border-black px-3 py-2">
        <div className="font-bold">{doc.entidade_nome}</div>
        {!consumidor && <div>NIF: {doc.entidade_nif}</div>}
        {doc.entidade_endereco && <div>{doc.entidade_endereco}</div>}
      </div>
      <table className="w-full border-collapse text-left">
        <thead>
          <tr className="border-b border-black">
            <th className="py-1">Ref.</th>
            <th className="py-1">Descrição</th>
            <th className="py-1 text-right">Quantidade</th>
            <th className="py-1 text-right">Conferido</th>
          </tr>
        </thead>
        <tbody>
          {d.linhas.map((l) => (
            <tr key={l.id} className="border-b border-dotted border-gray-400">
              <td className="py-1 font-mono">{l.referencia}</td>
              <td className="py-1">{l.descricao}</td>
              <td className="py-1 text-right">
                {qtd(l.quantidade)} {l.unidade ?? ""}
              </td>
              <td className="py-1 text-right">☐</td>
            </tr>
          ))}
        </tbody>
      </table>
      {doc.observacoes && <div className="mt-3">Observações: {doc.observacoes}</div>}
      <div className="mt-16 grid grid-cols-2 gap-16 text-center">
        <div className="border-t border-black pt-1">Entregou</div>
        <div className="border-t border-black pt-1">Recebeu (nome, data e assinatura)</div>
      </div>
      <div className="mt-6 text-center text-[10.5px]">Este documento não serve de factura.</div>
    </div>
  );
}

const ARIAL = { fontFamily: "Arial, Helvetica, sans-serif" };
const hora = (dh?: string | null) => (dh && dh.length >= 16 ? dh.slice(11, 16) : "");
const nifTexto = (nif: string) => (nif === "999999999" ? "Consumidor Final" : nif);
/** Linha de localização como no KIGEST: «Luanda - Angola». */
const local = (e: DocCompleto["empresa"]) => `${e.localidade || e.provincia || "Luanda"} - Angola`;
/** Parte do desconto que vem da percentagem de cada linha; o resto é o desconto global (repartido pelas linhas). */
function descontosLinha(d: DocCompleto) {
  return d.linhas.reduce((s, l) => s + Math.round((l.quantidade * l.preco_unitario * (l.desconto_pct || 0)) / 100), 0);
}
const ehRecibo = (classe: string) => classe === "RECIBO" || classe === "PAGAMENTO";
const naoServeFactura = (classe: string) => classe === "PROFORMA" || ehRecibo(classe) || classe === "TRANSPORTE";

/**
 * Talão para impressora térmica de 80 mm (POS80 / EPSON TM-T20II) ou de 58 mm, com o desenho do Fatura.fr3
 * do KIGEST POS: empresa ao centro, título, n.º, data e hora, cliente, linhas em duas filas, totais,
 * modo de pagamento, resumo de imposto, código de certificação, operador e «OBRIGADO VOLTE SEMPRE».
 */
export function Ticket({ d, via = "Original" }: { d: DocCompleto; via?: string }) {
  const doc = d.documento;
  const e = d.empresa;
  const t58 = useTalao58();
  const recibo = ehRecibo(doc.classe);
  const segunda = via.startsWith("2.ª");
  const outraVia = !segunda && via !== "Original";
  const cols = t58 ? "grid-cols-[22px_18px_1fr_26px_26px_1fr]" : "grid-cols-[30px_24px_1fr_36px_36px_1fr]";
  const cabecalho = (t: string) => <div className="mt-2 border-y border-black py-0.5 text-center">{t}</div>;
  return (
    <div className={`print-ticket mx-auto bg-white text-black ${t58 ? "talao-58 w-[220px] p-2 text-[9.5px] leading-[1.25]" : "w-[300px] p-3 text-[11px] leading-[1.3]"}`} style={ARIAL}>
      <div className="text-center">
        {e.logo && <img src={e.logo} alt="" className="mx-auto mb-1 max-h-14 max-w-[180px] object-contain grayscale" />}
        <div className="text-[13px] font-bold">{e.nome}</div>
        {e.designacao_comercial && e.designacao_comercial !== e.nome && <div>{e.designacao_comercial}</div>}
        {e.endereco && <div>{e.endereco}</div>}
        {e.telefone && <div>Tel.: {e.telefone}</div>}
        <div>NIF: {e.nif}</div>
        <div>{local(e)}</div>
      </div>
      {segunda && <div className="mt-1.5 text-center font-bold">2.ª via em conformidade com o original</div>}
      <div className="mt-2 flex items-baseline justify-between gap-2">
        <span className="text-[13px] font-bold">{doc.numero_doc}</span>
        <span className="text-right font-bold">{doc.tipo_desc}</span>
      </div>
      {outraVia && <div className="text-right">{via}</div>}
      <div className="flex justify-between">
        <span>Data: {data(doc.data_emissao)}</span>
        <span>Hora: {hora(doc.data_hora)}</span>
      </div>
      <div>Cliente : {doc.entidade_nome}</div>
      <div>NIF : {nifTexto(doc.entidade_nif)}</div>
      {/* nota de crédito/débito: origem e motivo; factura a crédito: vencimento (como no A4) */}
      {doc.doc_origem && <div>Ref.: {doc.doc_origem}</div>}
      {doc.motivo && <div>Motivo: {doc.motivo}</div>}
      {doc.data_vencimento && doc.data_vencimento.slice(0, 10) !== doc.data_emissao.slice(0, 10) && !recibo && (
        <div>
          {doc.classe === "PROFORMA" ? "Válido até" : "Vencimento"}: {data(doc.data_vencimento)}
        </div>
      )}
      {naoServeFactura(doc.classe) && <div className="mt-1 text-center">Este documento não serve de factura</div>}
      {doc.estado === "A" && <div className="my-1 text-center font-bold">*** DOCUMENTO ANULADO ***</div>}

      {!recibo && (
        <>
          <div className="mt-2 font-bold">Descrição</div>
          <div className={`grid ${cols} gap-x-1 border-b border-black pb-0.5 font-bold`}>
            <span>Qtd</span>
            <span>Un</span>
            <span className="text-right">P. Unit</span>
            <span className="text-right">Desc.%</span>
            <span className="text-right">Taxa%</span>
            <span className="text-right">Valor</span>
          </div>
          {d.linhas.map((l) => (
            <div key={l.id} className="mt-1">
              <div>{l.descricao}</div>
              <div className={`grid ${cols} gap-x-1`}>
                <span>{qtd(l.quantidade)}</span>
                <span className="truncate">{l.unidade ?? "un"}</span>
                <span className="text-right">{kzNum(l.preco_unitario)}</span>
                <span className="text-right">{l.desconto_pct ? pct(l.desconto_pct) : "0"}</span>
                <span className="text-right">{pct(l.taxa_iva_pct)}</span>
                <span className="text-right">{kzNum(l.total)}</span>
              </div>
              {l.motivo_isencao && (
                <div className="text-[9px]">
                  {l.motivo_isencao} {MOTIVO_TXT[l.motivo_isencao] ?? "Isento"}
                </div>
              )}
              {l.lote && <div className="text-[9px]">Lote {l.lote}</div>}
            </div>
          ))}
        </>
      )}
      {recibo && (
        <>
          <div className={`mt-2 grid grid-cols-[1fr_auto_auto] gap-x-2 border-b border-black pb-0.5 font-bold`}>
            <span>Documento</span>
            <span>Data</span>
            <span className="text-right">Valor</span>
          </div>
          {d.liquidacoes.map((l) => (
            <div key={l.id} className="grid grid-cols-[1fr_auto_auto] gap-x-2">
              <span>{l.numero_doc}</span>
              <span>{data(l.data_emissao)}</span>
              <span className="text-right">{kzNum(l.valor)}</span>
            </div>
          ))}
        </>
      )}

      <div className="mt-1.5 border-t border-black pt-1">
        {!recibo && (
          <>
            <Row k="Total sem Imposto" v={kzNum(doc.total_bruto)} />
            <Row k="Descontos" v={kzNum(doc.total_desconto)} />
            <Row k="Total Imposto" v={kzNum(doc.total_iva)} />
          </>
        )}
        <div className="flex justify-between text-[14px] font-bold">
          <span>Total ({doc.moeda || "AOA"})</span>
          <span>{kzNum(doc.total)}</span>
        </div>
        {doc.valor_entregue > 0 && <Row k="Valor Entregue" v={kzNum(doc.valor_entregue)} />}
        {doc.troco > 0 && <Row k="Troco" v={kzNum(doc.troco)} />}
      </div>
      {d.pagamentos.length > 0 && (
        <>
          {cabecalho("Modo de Pagamento")}
          {d.pagamentos.map((p) => (
            <Row key={p.modo_codigo} k={p.descricao} v={kzNum(p.valor + (p.modo_codigo === "NU" ? doc.troco : 0))} />
          ))}
        </>
      )}
      {!recibo && d.resumo_iva.length > 0 && (
        <>
          {cabecalho("Resumo de Imposto")}
          <div className="grid grid-cols-3 text-[10px]">
            <span>Taxa</span>
            <span className="text-center">Incidência</span>
            <span className="text-right">Total Imposto</span>
            {d.resumo_iva.map((r) => (
              <Fragment key={`${r.taxa}${r.motivo_isencao}`}>
                <span>
                  {pct(r.taxa)}
                  {r.motivo_isencao ? ` ${r.motivo_isencao}` : ""}
                </span>
                <span className="text-center">{kzNum(r.base)}</span>
                <span className="text-right">{kzNum(r.iva)}</span>
              </Fragment>
            ))}
          </div>
        </>
      )}
      {doc.classe === "VENDA" && <div className="mt-2 text-center text-[9px]">Os bens e/ou serviços foram colocados à disposição do adquirente na data e local do documento.</div>}
      {d.fe && <QrAgt d={d} tamanho="32mm" />}
      {rodapeAgt(d) && <div className="mt-2 text-center text-[10px]">{rodapeAgt(d)}</div>}
      <div className="mt-1.5">Operador: {doc.utilizador}</div>
      {doc.classe === "PROFORMA" && e.bancos && e.bancos.length > 0 && (
        <div className="mt-1 text-center text-[9.5px]">
          {e.bancos.map((b) => (
            <div key={b.iban}>
              {b.banco}: {b.iban}
            </div>
          ))}
        </div>
      )}
      {doc.texto_rodape && <div className="mt-1 text-center text-[10px]">{doc.texto_rodape}</div>}
      {e.rodape && <div className="mt-1 text-center text-[10px]">{e.rodape}</div>}
      <div className="mt-1.5 text-center">* OBRIGADO VOLTE SEMPRE *</div>
    </div>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex justify-between">
      <span>{k}</span>
      <span>{v}</span>
    </div>
  );
}

const OPCOES_PADRAO = { mostrar_bancos: true, valor_extenso: true, assinaturas: false, mostrar_operador: true };

/** Título de secção do rodapé do A4 (RESUMO DE IMPOSTO, RETENÇÕES, COORDENADAS BANCÁRIAS…). */
function Seccao({ titulo, children }: { titulo: string; children: ReactNode }) {
  return (
    <div className="mt-2">
      <div className="border-b border-black text-[8.5px] font-bold uppercase">{titulo}</div>
      {children}
    </div>
  );
}

/**
 * Documento em A4 com o desenho dos modelos do KIGEST GC (FaturaVenda, FaturaRecibo, FaturaProforma, NotaCredito,
 * ReciboClienteNew…): logótipo e empresa à esquerda; n.º do documento e «Exmo(s) Senhor(es)» à direita; faixa
 * com contribuinte, cliente, datas, condição de pagamento, origem e moeda; linhas; e no fundo da página o resumo
 * de imposto, retenções e coordenadas bancárias à esquerda e os totais com o TOTAL DO DOCUMENTO à direita.
 * Com o formato A5 do tipo de documento, o mesmo desenho reduzido para meia folha.
 */
export function A4({ d, via = "Original" }: { d: DocCompleto; via?: string }) {
  const doc = d.documento;
  const e = d.empresa;
  const o = d.opcoes ?? OPCOES_PADRAO;
  const recibo = ehRecibo(doc.classe);
  const proforma = doc.classe === "PROFORMA";
  const compras = doc.grupo === "COMPRAS";
  const stock = doc.classe === "STOCK";
  const a5 = doc.formato === "A5";
  const moeda = doc.moeda || "AOA";
  const porLinha = Math.min(descontosLinha(d), doc.total_desconto);
  const faixa: [string, ReactNode][] = [
    [compras ? "N/ Nº Contrib." : "V/ Nº Contrib.", nifTexto(doc.entidade_nif)],
    [compras ? "Fornecedor" : "Cliente", doc.entidade_id ?? "—"],
    [compras ? "V/ Documento" : "V/ Refª", doc.doc_externo || doc.referencia || "—"],
    ["Data de Emissão", data(doc.data_emissao)],
    ["Hora", hora(doc.data_hora)],
  ];
  if (!recibo && !stock) faixa.push([proforma ? "Validade" : "Data de Vencimento", data(doc.data_vencimento || doc.data_emissao)]);
  if (!recibo && !stock && doc.prazo_desc) faixa.push(["Condição de Pagamento", doc.prazo_desc]);
  if (doc.doc_origem) faixa.push(["Documento de Origem", doc.doc_origem]);
  // como no KIGEST, o armazém só aparece nos documentos de stock (entradas, saídas, transferências)
  if (doc.armazem && stock) faixa.push(["Armazém", `${doc.armazem}${doc.armazem_destino ? ` → ${doc.armazem_destino}` : ""}`]);
  faixa.push(["Moeda", moeda]);

  const totais: [string, number][] = recibo
    ? []
    : [
        ["Total Ilíquido", doc.total_bruto],
        ["Descontos por Linha", porLinha],
        ["Desconto Global", doc.total_desconto - porLinha],
        ["Total Líquido", doc.total_liquido],
        ["Imposto (IVA)", doc.total_iva],
        ["Total com Imposto", doc.total],
        ["Retenção", doc.total_retencao],
      ];

  return (
    <div
      className="mx-auto flex min-h-[272mm] w-[190mm] flex-col bg-white p-1 text-[10px] leading-snug text-black"
      style={{ ...ARIAL, ...(a5 ? { zoom: 134 / 190 } : {}) }}
    >
      {/* ---------- cabeçalho ---------- */}
      <div className="grid grid-cols-2 gap-8">
        <div>
          {e.logo ? <img src={e.logo} alt="" className="mb-2 max-h-[30mm] max-w-[68mm] object-contain object-left" /> : <div className="h-2" />}
          <div className="text-[11.5px] font-bold">{e.nome}</div>
          {e.designacao_comercial && e.designacao_comercial !== e.nome && <div>{e.designacao_comercial}</div>}
          <div>NIF: {e.nif}</div>
          {e.endereco && <div>{e.endereco}</div>}
          {e.telefone && <div>Tel. : {e.telefone}</div>}
          {e.email && <div>Email: {e.email}</div>}
          <div>{local(e)}</div>
        </div>
        <div className="flex flex-col">
          <div className="text-right text-[12px] font-bold">{via}</div>
          <div className="mt-3 text-[11px] font-bold uppercase">{doc.tipo_desc}</div>
          <div className="text-[17px] font-bold leading-tight">{doc.numero_doc}</div>
          {naoServeFactura(doc.classe) && <div className="text-[9.5px]">Este documento não serve de factura</div>}
          {doc.estado === "A" && <div className="mt-1 text-[12px] font-bold">DOCUMENTO ANULADO</div>}
          <div className="mt-3">{stock ? "" : "Exmo(s) Senhor(es)"}</div>
          {!stock && (
            <>
              <div className="font-bold">{doc.entidade_nome}</div>
              {doc.entidade_endereco && <div>{doc.entidade_endereco}</div>}
              {doc.entidade_localidade && <div>{doc.entidade_localidade}</div>}
            </>
          )}
          {doc.observacoes && <div className="mt-2 text-[9.5px] italic">{doc.observacoes}</div>}
        </div>
      </div>

      {/* ---------- faixa de dados ---------- */}
      <table className="mt-4 w-full border-collapse">
        <thead>
          <tr className="border-b border-black text-left align-bottom text-[8.5px] font-bold uppercase">
            {faixa.map(([k]) => (
              <th key={k} className="pb-0.5 pr-2 font-bold">
                {k}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          <tr className="align-top">
            {faixa.map(([k, v]) => (
              <td key={k} className="pr-2 pt-0.5">
                {v}
              </td>
            ))}
          </tr>
        </tbody>
      </table>

      {doc.motivo && (
        <div className="mt-3">
          <b>Motivo de emissão:</b> {doc.motivo}
        </div>
      )}

      {doc.transporte && (
        <div className="mt-3 grid grid-cols-2 gap-x-6 gap-y-0.5 border-y border-black py-1 text-[9.5px]">
          <div>
            <b>Local de carga:</b> {doc.transporte.carga_local || "—"} {doc.transporte.carga_data ? `· ${doc.transporte.carga_data.replace("T", " ")}` : ""}
          </div>
          <div>
            <b>Local de descarga:</b> {doc.transporte.descarga_local || "—"} {doc.transporte.descarga_data ? `· ${doc.transporte.descarga_data.replace("T", " ")}` : ""}
          </div>
          <div>
            <b>Viatura:</b> {doc.transporte.matricula || "—"}
          </div>
          <div>
            <b>Motorista:</b> {doc.transporte.motorista || "—"}
          </div>
        </div>
      )}

      {/* ---------- linhas ---------- */}
      {!recibo && (
        <table className="mt-4 w-full border-collapse">
          <thead>
            <tr className="border-y border-black text-left text-[8.5px] uppercase">
              <th className="w-[22mm] py-0.5 font-bold">Referência</th>
              <th className="font-bold">Descrição</th>
              <th className="text-right font-bold">Qtd</th>
              <th className="pl-1 font-bold">Un</th>
              <th className="text-right font-bold">Preço Unit.</th>
              <th className="text-right font-bold">Desc.(%)</th>
              <th className="text-right font-bold">Taxa(%)</th>
              <th className="text-right font-bold">Total</th>
            </tr>
          </thead>
          <tbody className="text-[9.5px]">
            {d.linhas.map((l) => (
              <Fragment key={l.id}>
                <tr className="align-top">
                  <td className="pr-2 pt-0.5">{l.referencia}</td>
                  <td className="pr-2 pt-0.5">{l.descricao}</td>
                  <td className="pt-0.5 text-right">{qtd(l.quantidade)}</td>
                  <td className="pl-1 pt-0.5">{l.unidade ?? "un"}</td>
                  <td className="pt-0.5 text-right">{kzNum(l.preco_unitario)}</td>
                  <td className="pt-0.5 text-right">{l.desconto_pct ? pct(l.desconto_pct) : "0,00"}</td>
                  <td className="pt-0.5 text-right">{pct(l.taxa_iva_pct)}</td>
                  <td className="pt-0.5 text-right">{kzNum(l.valor_liquido)}</td>
                </tr>
                {(l.motivo_isencao || l.lote) && (
                  <tr>
                    <td />
                    <td colSpan={7} className="text-[8.5px]">
                      {l.motivo_isencao && `${l.motivo_isencao} ${MOTIVO_TXT[l.motivo_isencao] ?? "Isento"}`}
                      {l.motivo_isencao && l.lote && " · "}
                      {l.lote && `Lote ${l.lote}${l.validade ? ` · validade ${data(l.validade)}` : ""}`}
                    </td>
                  </tr>
                )}
              </Fragment>
            ))}
          </tbody>
        </table>
      )}

      {recibo && (
        <table className="mt-4 w-full border-collapse">
          <thead>
            <tr className="border-y border-black text-left text-[8.5px] uppercase">
              <th className="py-0.5 font-bold">Doc. de Referência</th>
              <th className="font-bold">Data de Emissão</th>
              <th className="text-right font-bold">Total</th>
              <th className="text-right font-bold">Liquidado</th>
            </tr>
          </thead>
          <tbody className="text-[9.5px]">
            {d.liquidacoes.map((l) => (
              <tr key={l.id}>
                <td className="pt-0.5">{l.numero_doc}</td>
                <td className="pt-0.5">{data(l.data_emissao)}</td>
                <td className="pt-0.5 text-right">{kzNum(l.total)}</td>
                <td className="pt-0.5 text-right">{kzNum(l.valor)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {/* ---------- rodapé (no fundo da página, como o PageFooter do KIGEST) ---------- */}
      <div className="mt-auto pt-6">
        <div className="flex items-end justify-between gap-4 border-b border-black pb-0.5 text-[9.5px]">
          <span>{rodapeAgt(d) ?? ""}</span>
          {o.mostrar_operador && <span className="shrink-0">Operador : {doc.utilizador}</span>}
        </div>
        <div className="grid grid-cols-2 gap-8">
          <div>
            {!recibo && d.resumo_iva.length > 0 && (
              <Seccao titulo="Resumo de imposto">
                <table className="w-full border-collapse text-[9px]">
                  <thead>
                    <tr className="text-[8px] uppercase">
                      <th className="text-left font-bold">Descrição</th>
                      <th className="text-right font-bold">Taxa</th>
                      <th className="text-right font-bold">Incidência</th>
                      <th className="text-right font-bold">Imposto</th>
                    </tr>
                  </thead>
                  <tbody>
                    {d.resumo_iva.map((r) => (
                      <tr key={`${r.taxa}${r.motivo_isencao}`}>
                        <td>{r.motivo_isencao ? `${r.motivo_isencao} ${MOTIVO_TXT[r.motivo_isencao] ?? "Isento"}` : "IVA"}</td>
                        <td className="text-right">{pct(r.taxa)}</td>
                        <td className="text-right">{kzNum(r.base)}</td>
                        <td className="text-right">{kzNum(r.iva)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </Seccao>
            )}
            {d.retencoes?.length > 0 && (
              <Seccao titulo="Retenções">
                <table className="w-full border-collapse text-[9px]">
                  <thead>
                    <tr className="text-[8px] uppercase">
                      <th className="text-left font-bold">Tipo</th>
                      <th className="text-right font-bold">Percent.(%)</th>
                      <th className="text-right font-bold">Retenção</th>
                    </tr>
                  </thead>
                  <tbody>
                    {d.retencoes.map((r) => (
                      <tr key={r.codigo}>
                        <td>{r.descricao || r.codigo}</td>
                        <td className="text-right">{pct(r.percentagem)}</td>
                        <td className="text-right">{kzNum(r.valor)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </Seccao>
            )}
            {d.pagamentos.length > 0 && (
              <Seccao titulo="Método de pagamento">
                {d.pagamentos.map((p) => (
                  <div key={p.modo_codigo} className="flex justify-between text-[9px]">
                    <span>{p.descricao}</span>
                    <span>{kzNum(p.valor)}</span>
                  </div>
                ))}
              </Seccao>
            )}
            {o.mostrar_bancos && e.bancos && e.bancos.length > 0 && !recibo && !stock && !compras && (
              <Seccao titulo="Coordenadas bancárias">
                <table className="w-full border-collapse text-[9px]">
                  <thead>
                    <tr className="text-[8px] uppercase">
                      <th className="text-left font-bold">Banco</th>
                      <th className="text-left font-bold">Conta</th>
                      <th className="text-left font-bold">IBAN</th>
                    </tr>
                  </thead>
                  <tbody>
                    {e.bancos.map((b) => (
                      <tr key={b.iban}>
                        <td>{b.banco}</td>
                        <td>{b.conta ?? ""}</td>
                        <td>{b.iban}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </Seccao>
            )}
          </div>
          <div>
            <div className="mt-2">
              {totais.map(([k, v]) => (
                <div key={k} className="flex justify-between border-b border-black/40 py-[1px] text-[9px]">
                  <span className="font-bold uppercase">{k}</span>
                  <span>{kzNum(v)}</span>
                </div>
              ))}
            </div>
            <div className="mt-3 flex items-start justify-between gap-3">
              <div className="text-[12px] font-bold uppercase leading-tight">
                {recibo ? "Liquidado" : "Total do documento"}
                <br />({moeda})
              </div>
              <div className="text-[14px] font-bold">{kzNum(doc.total)}</div>
            </div>
            {d.retencoes?.length > 0 && (
              <div className="flex justify-between text-[10px] font-bold">
                <span>Total a pagar</span>
                <span>{kzNum(doc.total - doc.total_retencao)}</span>
              </div>
            )}
            {doc.moeda && doc.moeda !== "AOA" && doc.total_moeda !== null && (
              <div className="mt-1 text-right text-[9.5px]">
                Total em {doc.moeda}: <b>{kzNum(doc.total_moeda)}</b> · câmbio {String(doc.cambio).replace(".", ",")} Kz
              </div>
            )}
            {!!d.custos?.length && (
              <div className="mt-1 text-right text-[9px]">Custos adicionais (no custo dos artigos): {kzNum(d.custos.reduce((s, c) => s + c.valor, 0))}</div>
            )}
            {(o.valor_extenso || recibo) && !stock && (
              <div className="mt-2 text-[9.5px]">
                {doc.prefixo_extenso || "São"}: {extenso(doc.total)}
              </div>
            )}
          </div>
        </div>

        {(o.assinaturas || doc.tipo_codigo === "NC") && (
          <div className="mt-10 grid grid-cols-2 gap-16 text-center text-[9.5px]">
            <div className="border-t border-black pt-0.5 font-bold uppercase">{recibo && doc.classe === "PAGAMENTO" ? "Recebi" : "Assinatura do cliente"}</div>
            <div className="border-t border-black pt-0.5">{o.assinaturas ? e.designacao_comercial || e.nome : ""}</div>
          </div>
        )}

        {d.fe && <QrAgt d={d} tamanho="28mm" />}
        <div className="mt-3 flex items-end justify-between gap-4 text-[9px]">
          <div>
            {doc.texto_rodape && <div className="font-medium">{doc.texto_rodape}</div>}
            {doc.classe === "VENDA" && <div>Os bens e/ou serviços foram colocados à disposição do adquirente na data e local do documento.</div>}
            {e.rodape && <div>{e.rodape}</div>}
          </div>
          <div className="shrink-0 text-[8px]">Processado por KanGest</div>
        </div>
      </div>
    </div>
  );
}

/** Renderiza o conteúdo no #print-area e chama a impressão do sistema. */
export function PrintPortal({ children }: { children: ReactNode }) {
  return createPortal(<div id="print-area">{children}</div>, document.body);
}

/** QR code da Facturação Electrónica da AGT (consulta do documento no Quiosque AGT). */
function QrAgt({ d, tamanho }: { d: DocCompleto; tamanho: string }) {
  if (!d.fe) return null;
  const q = qrcode(urlFacturaAgt(d.fe.nif, d.documento.numero_doc), "M");
  const lado = q.tamanho + 8;
  return (
    <div className="my-2 flex flex-col items-center text-center text-[9.5px]">
      <svg viewBox={`0 0 ${lado} ${lado}`} style={{ width: tamanho, height: tamanho }} shapeRendering="crispEdges" role="img" aria-label="QR code da AGT">
        <rect width={lado} height={lado} fill="#fff" />
        <path d={qrSvgPath(q)} fill="#000" />
      </svg>
      <div>{d.fe.estado === "VALIDO" ? "Documento validado pela AGT (facturação electrónica)" : "Documento comunicado à AGT (facturação electrónica)"}</div>
    </div>
  );
}
