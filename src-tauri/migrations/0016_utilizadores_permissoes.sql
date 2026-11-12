-- Módulo de utilizadores, perfis de acesso e autenticação

-- Perfis de acesso: os três de origem e os criados pela empresa (Armazém, Contabilista…).
-- «nivel» é o acesso por escalão (1 caixa · 2 gestor · 3 administrador); «permissoes» é a lista
-- por operação (JSON). NULL = a lista de origem do perfil.
CREATE TABLE perfis (
    codigo TEXT PRIMARY KEY,
    descricao TEXT NOT NULL,
    nivel INTEGER NOT NULL DEFAULT 1,
    permissoes TEXT,
    sistema INTEGER NOT NULL DEFAULT 0
);
INSERT INTO perfis (codigo, descricao, nivel, sistema) VALUES
    ('ADMIN', 'Administrador', 3, 1),
    ('GESTOR', 'Gestor', 2, 1),
    ('CAIXA', 'Caixa (POS)', 1, 1);

-- Segurança da conta
ALTER TABLE utilizadores ADD COLUMN mudar_senha INTEGER NOT NULL DEFAULT 0;   -- obrigar a mudar a senha no próximo acesso
ALTER TABLE utilizadores ADD COLUMN tentativas INTEGER NOT NULL DEFAULT 0;    -- tentativas falhadas seguidas
ALTER TABLE utilizadores ADD COLUMN bloqueado_ate TEXT;                       -- bloqueio temporário por tentativas falhadas
ALTER TABLE utilizadores ADD COLUMN ultimo_acesso TEXT;
ALTER TABLE utilizadores ADD COLUMN senha_alterada_em TEXT;

-- Utilizadores importados do KIGEST que ainda não entraram têm a senha provisória da importação
UPDATE utilizadores SET mudar_senha = 1
 WHERE id_origem IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM auditoria a WHERE a.utilizador_id = utilizadores.id AND a.acao = 'LOGIN');

-- Registo de acessos: entradas, saídas, tentativas falhadas, bloqueios e mudanças de senha
CREATE TABLE acessos (
    id INTEGER PRIMARY KEY,
    data_hora TEXT NOT NULL DEFAULT (datetime('now','localtime')),
    utilizador_id INTEGER REFERENCES utilizadores(id),
    login TEXT,
    evento TEXT NOT NULL,
    posto_id INTEGER,
    detalhe TEXT
);
CREATE INDEX idx_acessos_data ON acessos(data_hora);
CREATE INDEX idx_acessos_utilizador ON acessos(utilizador_id);
CREATE INDEX IF NOT EXISTS idx_auditoria_data ON auditoria(data_hora);

-- Política de segurança
INSERT OR IGNORE INTO config (chave, valor) VALUES
    ('seguranca.senha_min', '6'),
    ('seguranca.tentativas', '5'),
    ('seguranca.bloqueio_min', '15'),
    ('seguranca.inatividade_min', '0'),
    ('seguranca.login_pin', '0');

-- Permissões novas que substituem o acesso por escalão de gestor: quem tem lista própria continua a tê-las
UPDATE utilizadores
   SET permissoes = json_insert(json_insert(permissoes, '$[#]', 'sistema.tabelas'), '$[#]', 'tesouraria.regularizar')
 WHERE perfil = 'GESTOR' AND permissoes IS NOT NULL AND json_valid(permissoes);
