import type { ReactNode } from "react";
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

/** Talão para impressora térmica de 80 mm (ex.: POS80 / EPSON TM-T20II) ou de 58 mm, conforme o posto. */
export function Ticket({ d, via = "Original" }: { d: DocCompleto; via?: string }) {
  const doc = d.documento;
  const e = d.empresa;
  const t58 = useTalao58();
  return (
    <div className={`print-ticket mx-auto bg-white font-mono text-black ${t58 ? "talao-58 w-[220px] p-2 text-[10px] leading-[1.3]" : "w-[300px] p-3 text-[11px] leading-[1.35]"}`}>
      <div className="text-center">
        {e.logo && <img src={e.logo} alt="" className="mx-auto mb-1 max-h-14 max-w-[180px] object-contain grayscale" />}
        <div className="text-[13px] font-bold">{e.designacao_comercial || e.nome}</div>
        {e.designacao_comercial && <div>{e.nome}</div>}
        <div>NIF: {e.nif}</div>
        {e.endereco && <div>{e.endereco}</div>}
        {e.telefone && <div>Tel: {e.telefone}</div>}
      </div>
      <div className="my-2 border-t border-dashed border-black" />
      <div className="text-center font-bold">
        {doc.tipo_desc} {doc.numero_doc}
      </div>
      <div className="text-center">{via}</div>
      <div className="mt-1">Data: {dataHora(doc.data_hora)}</div>
      <div>
        Cliente: {doc.entidade_nome}
        <br />
        NIF: {doc.entidade_nif === "999999999" ? "Consumidor Final" : doc.entidade_nif}
      </div>
      {/* nota de crédito/débito: origem e motivo; factura a crédito: vencimento (como no A4) */}
      {doc.doc_origem && <div>Ref.: {doc.doc_origem}</div>}
      {doc.motivo && <div>Motivo: {doc.motivo}</div>}
      {doc.data_vencimento && doc.data_vencimento.slice(0, 10) !== doc.data_emissao.slice(0, 10) && doc.classe !== "RECIBO" && (
        <div>
          {doc.classe === "PROFORMA" ? "Válido até" : "Vencimento"}: {data(doc.data_vencimento)}
        </div>
      )}
      {doc.estado === "A" && <div className="my-1 text-center font-bold">*** DOCUMENTO ANULADO ***</div>}
      <div className="my-2 border-t border-dashed border-black" />
      {d.linhas.map((l) => (
        <div key={l.id} className="mb-1">
          <div>{l.descricao}</div>
          <div className="flex justify-between">
            <span>
              {qtd(l.quantidade)} x {kzNum(l.preco_unitario)} {l.desconto_pct ? `\u2212${pct(l.desconto_pct)}` : ""} ({pct(l.taxa_iva_pct)}
              {l.motivo_isencao ? ` ${l.motivo_isencao}` : ""})
            </span>
            <span>{kzNum(l.total)}</span>
          </div>
        </div>
      ))}
      <div className="my-2 border-t border-dashed border-black" />
      <Row k="Total Ilíquido" v={kzNum(doc.total_bruto)} />
      {doc.total_desconto > 0 && <Row k="Descontos" v={`\u2212${kzNum(doc.total_desconto)}`} />}
      <Row k="Total IVA" v={kzNum(doc.total_iva)} />
      <div className="mt-1 flex justify-between text-[14px] font-bold">
        <span>TOTAL Kz</span>
        <span>{kzNum(doc.total)}</span>
      </div>
      {d.pagamentos.length > 0 && (
        <div className="mt-1">
          {d.pagamentos.map((p) => (
            <Row key={p.modo_codigo} k={p.descricao} v={kzNum(p.valor + (p.modo_codigo === "NU" ? doc.troco : 0))} />
          ))}
          {doc.troco > 0 && <Row k="Troco" v={kzNum(doc.troco)} />}
        </div>
      )}
      <div className="my-2 border-t border-dashed border-black" />
      <div className="text-[10px]">
        {d.resumo_iva.map((r) => (
          <div key={`${r.taxa}${r.motivo_isencao}`} className="flex justify-between">
            <span>
              IVA {pct(r.taxa)} {r.motivo_isencao ? `(${r.motivo_isencao})` : ""}
            </span>
            <span>
              Base {kzNum(r.base)} · IVA {kzNum(r.iva)}
            </span>
          </div>
        ))}
        {d.resumo_iva.filter((r) => r.motivo_isencao).map((r) => (
          <div key={`m${r.motivo_isencao}`}>
            {r.motivo_isencao}: {MOTIVO_TXT[r.motivo_isencao!] ?? "Isento"}
          </div>
        ))}
      </div>
      <div className="mt-2 text-center text-[10px]">
        Os bens e/ou serviços foram colocados à disposição do adquirente na data e local do documento.
      </div>
      <div className="mt-1 text-center text-[10px]">Operador: {doc.utilizador}</div>
      {d.fe && <QrAgt d={d} tamanho="32mm" />}
      {rodapeAgt(d) && <div className="mt-1 text-center text-[10px] font-bold">{rodapeAgt(d)}</div>}
      {doc.texto_rodape && <div className="mt-1 text-center text-[10px]">{doc.texto_rodape}</div>}
      {e.rodape && <div className="mt-1 text-center text-[10px]">{e.rodape}</div>}
      <div className="mt-2 text-center">Obrigado pela preferência!</div>
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

/** Documento em A4 (factura, proforma, recibo, guias). */
const OPCOES_PADRAO = { mostrar_bancos: true, valor_extenso: false, assinaturas: false, mostrar_operador: true };

/** Documento em A4; com o formato A5 do tipo de documento, o mesmo desenho reduzido para meia folha. */
export function A4({ d, via = "Original" }: { d: DocCompleto; via?: string }) {
  const doc = d.documento;
  const e = d.empresa;
  const o = d.opcoes ?? OPCOES_PADRAO;
  const recibo = doc.classe === "RECIBO" || doc.classe === "PAGAMENTO";
  const a5 = doc.formato === "A5";
  return (
    <div className="mx-auto w-[190mm] bg-white p-2 text-[11.5px] leading-snug text-black" style={a5 ? { zoom: 134 / 190 } : undefined}>
      <div className="flex items-start justify-between border-b-2 border-black pb-3">
        <div className="flex items-start gap-4">
          {e.logo && <img src={e.logo} alt="" className="max-h-20 max-w-[45mm] object-contain" />}
          <div>
          <div className="text-[18px] font-bold">{e.designacao_comercial || e.nome}</div>
          {e.designacao_comercial && <div>{e.nome}</div>}
          <div>NIF: {e.nif}</div>
          <div>{[e.endereco, e.localidade, e.provincia].filter(Boolean).join(", ")}</div>
          <div>{[e.telefone && `Tel: ${e.telefone}`, e.email].filter(Boolean).join(" · ")}</div>
          </div>
        </div>
        <div className="text-right">
          <div className="text-[16px] font-bold uppercase">{doc.tipo_desc}</div>
          <div className="text-[15px] font-semibold">{doc.numero_doc}</div>
          <div className="mt-1">{via}</div>
          {doc.estado === "A" && <div className="mt-1 font-bold">ANULADO</div>}
        </div>
      </div>

      <div className="mt-4 grid grid-cols-2 gap-6">
        <div className="rounded border border-black/40 p-3">
          <div className="text-[10px] uppercase text-black/60">Cliente / Entidade</div>
          <div className="font-semibold">{doc.entidade_nome}</div>
          <div>NIF: {doc.entidade_nif === "999999999" ? "Consumidor Final" : doc.entidade_nif}</div>
          {doc.entidade_endereco && <div>{doc.entidade_endereco}</div>}
        </div>
        <div className="grid grid-cols-2 gap-x-4 gap-y-1 self-start">
          <span className="text-black/60">Data de emissão</span>
          <span>{data(doc.data_emissao)}</span>
          {doc.data_vencimento && !recibo && (
            <>
              <span className="text-black/60">{doc.classe === "PROFORMA" ? "Válido até" : "Vencimento"}</span>
              <span>{data(doc.data_vencimento)}</span>
            </>
          )}
          {doc.prazo_desc && !recibo && (
            <>
              <span className="text-black/60">Prazo de pagamento</span>
              <span>{doc.prazo_desc}</span>
            </>
          )}
          {doc.referencia && (
            <>
              <span className="text-black/60">V/ Referência</span>
              <span>{doc.referencia}</span>
            </>
          )}
          {doc.doc_externo && (
            <>
              <span className="text-black/60">Doc. do fornecedor</span>
              <span>{doc.doc_externo}</span>
            </>
          )}
          {doc.doc_origem && (
            <>
              <span className="text-black/60">Documento origem</span>
              <span>{doc.doc_origem}</span>
            </>
          )}
          {doc.armazem && (
            <>
              <span className="text-black/60">Armazém</span>
              <span>
                {doc.armazem}
                {doc.armazem_destino ? ` → ${doc.armazem_destino}` : ""}
              </span>
            </>
          )}
          {o.mostrar_operador && (
            <>
              <span className="text-black/60">Operador</span>
              <span>{doc.utilizador}</span>
            </>
          )}
        </div>
      </div>

      {doc.motivo && (
        <div className="mt-3 rounded border border-black/30 px-3 py-1.5">
          <b>Motivo de emissão:</b> {doc.motivo}
        </div>
      )}

      {doc.transporte && (
        <div className="mt-3 grid grid-cols-2 gap-x-6 gap-y-0.5 rounded border border-black/30 px-3 py-2 text-[10.5px]">
          <div>
            <b>Carga:</b> {doc.transporte.carga_local || "—"} {doc.transporte.carga_data ? `· ${doc.transporte.carga_data.replace("T", " ")}` : ""}
          </div>
          <div>
            <b>Descarga:</b> {doc.transporte.descarga_local || "—"} {doc.transporte.descarga_data ? `· ${doc.transporte.descarga_data.replace("T", " ")}` : ""}
          </div>
          <div>
            <b>Viatura:</b> {doc.transporte.matricula || "—"}
          </div>
          <div>
            <b>Motorista:</b> {doc.transporte.motorista || "—"}
          </div>
        </div>
      )}

      {!recibo && (
        <table className="mt-5 w-full border-collapse">
          <thead>
            <tr className="border-b border-black text-left text-[10.5px] uppercase">
              <th className="py-1.5">Ref.</th>
              <th>Descrição</th>
              <th className="text-right">Qtd</th>
              <th className="text-right">Preço Unit.</th>
              <th className="text-right">Desc.</th>
              <th className="text-right">IVA</th>
              <th className="text-right">Total</th>
            </tr>
          </thead>
          <tbody>
            {d.linhas.map((l) => (
              <tr key={l.id} className="border-b border-black/15 align-top">
                <td className="py-1.5 pr-2">{l.referencia}</td>
                <td className="pr-2">
                  {l.descricao}
                  {l.lote && (
                    <div className="text-[10px] text-black/70">
                      Lote {l.lote}
                      {l.validade ? ` · validade ${data(l.validade)}` : ""}
                    </div>
                  )}
                </td>
                <td className="text-right">
                  {qtd(l.quantidade)} {l.unidade}
                </td>
                <td className="text-right">{kzNum(l.preco_unitario)}</td>
                <td className="text-right">{l.desconto_pct ? pct(l.desconto_pct) : ""}</td>
                <td className="text-right">
                  {pct(l.taxa_iva_pct)}
                  {l.motivo_isencao ? ` ${l.motivo_isencao}` : ""}
                </td>
                <td className="text-right">{kzNum(l.valor_liquido)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {recibo && (
        <div className="mt-5">
          <p>
            {doc.classe === "PAGAMENTO" ? "Pagámos a" : "Recebemos de"} <b>{doc.entidade_nome}</b> a quantia de <b>{kzNum(doc.total)} Kz</b> ({extenso(doc.total)}),
            referente a:
          </p>
          <table className="mt-3 w-full border-collapse">
            <thead>
              <tr className="border-b border-black text-left text-[10.5px] uppercase">
                <th className="py-1.5">Documento</th>
                <th>Data</th>
                <th className="text-right">Total</th>
                <th className="text-right">Valor pago</th>
              </tr>
            </thead>
            <tbody>
              {d.liquidacoes.map((l) => (
                <tr key={l.id} className="border-b border-black/15">
                  <td className="py-1.5">{l.numero_doc}</td>
                  <td>{data(l.data_emissao)}</td>
                  <td className="text-right">{kzNum(l.total)}</td>
                  <td className="text-right">{kzNum(l.valor)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div className="mt-5 flex justify-between gap-6">
        <div className="flex-1">
          {!recibo && d.resumo_iva.length > 0 && (
            <table className="w-full border-collapse text-[10.5px]">
              <thead>
                <tr className="border-b border-black text-left uppercase">
                  <th className="py-1">Quadro de IVA</th>
                  <th className="text-right">Incidência</th>
                  <th className="text-right">Imposto</th>
                </tr>
              </thead>
              <tbody>
                {d.resumo_iva.map((r) => (
                  <tr key={`${r.taxa}${r.motivo_isencao}`}>
                    <td className="py-0.5">
                      IVA {pct(r.taxa)}
                      {r.motivo_isencao ? ` — ${r.motivo_isencao} ${MOTIVO_TXT[r.motivo_isencao] ?? ""}` : ""}
                    </td>
                    <td className="text-right">{kzNum(r.base)}</td>
                    <td className="text-right">{kzNum(r.iva)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {d.pagamentos.length > 0 && (
            <div className="mt-3 text-[10.5px]">
              <b>Pagamento:</b> {d.pagamentos.map((p) => `${p.descricao} ${kzNum(p.valor)}`).join(" · ")}
            </div>
          )}
          {doc.observacoes && <div className="mt-3 text-[10.5px]"><b>Obs.:</b> {doc.observacoes}</div>}
        </div>
        <div className="w-[70mm]">
          {!recibo && (
            <>
              <TotRow k="Total Ilíquido" v={kzNum(doc.total_bruto)} />
              <TotRow k="Descontos" v={kzNum(doc.total_desconto)} />
              <TotRow k="Total Líquido" v={kzNum(doc.total_liquido)} />
              <TotRow k="Total IVA" v={kzNum(doc.total_iva)} />
            </>
          )}
          <div className="mt-1 flex justify-between border-t-2 border-black pt-1.5 text-[15px] font-bold">
            <span>Total (Kz)</span>
            <span>{kzNum(doc.total)}</span>
          </div>
          {doc.moeda && doc.moeda !== "AOA" && doc.total_moeda !== null && (
            <div className="mt-1 text-right text-[10.5px]">
              Total em {doc.moeda}: <b>{kzNum(doc.total_moeda)}</b> · câmbio {String(doc.cambio).replace(".", ",")} Kz
            </div>
          )}
          {!!d.custos?.length && (
            <div className="mt-1 text-right text-[10.5px] text-black/70">
              Custos adicionais (no custo dos artigos): {kzNum(d.custos.reduce((s, c) => s + c.valor, 0))}
            </div>
          )}
          {d.retencoes?.length > 0 && (
            <>
              {d.retencoes.map((r) => (
                <TotRow key={r.codigo} k={`Retenção ${r.codigo} (${pct(r.percentagem)})`} v={`-${kzNum(r.valor)}`} />
              ))}
              <div className="mt-1 flex justify-between border-t border-black pt-1 text-[13px] font-bold">
                <span>Total a pagar (Kz)</span>
                <span>{kzNum(doc.total - doc.total_retencao)}</span>
              </div>
            </>
          )}
        </div>
      </div>

      {o.valor_extenso && !recibo && (
        <div className="mt-3 text-[10.5px]">
          <b>{doc.prefixo_extenso || "Valor por extenso"}:</b> {extenso(doc.total)}
        </div>
      )}

      {o.mostrar_bancos && e.bancos && e.bancos.length > 0 && !recibo && (
        <div className="mt-4 text-[10px]">
          <b>Coordenadas bancárias:</b> {e.bancos.map((b) => `${b.banco} — IBAN ${b.iban}`).join(" · ")}
        </div>
      )}

      {o.assinaturas && (
        <div className="mt-12 grid grid-cols-2 gap-16 text-center text-[10.5px]">
          <div className="border-t border-black pt-1">{recibo ? (doc.classe === "PAGAMENTO" ? "Recebi" : "O Cliente") : "O Cliente"}</div>
          <div className="border-t border-black pt-1">{e.designacao_comercial || e.nome}</div>
        </div>
      )}

      <div className="mt-6 border-t border-black/30 pt-2 text-center text-[10px]">
        {doc.texto_rodape && <div className="font-medium">{doc.texto_rodape}</div>}
        {doc.classe === "VENDA" && <div>Os bens e/ou serviços foram colocados à disposição do adquirente na data e local do documento.</div>}
        {doc.classe === "PROFORMA" && <div>Este documento não serve de factura.</div>}
        {d.fe && <QrAgt d={d} tamanho="30mm" />}
        {rodapeAgt(d) && <div className="font-semibold">{rodapeAgt(d)}</div>}
        {e.rodape && <div>{e.rodape}</div>}
      </div>
    </div>
  );
}

function TotRow({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex justify-between py-0.5">
      <span>{k}</span>
      <span>{v}</span>
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
