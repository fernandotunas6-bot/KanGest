// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Motor central de dados: fontes declarativas (`fontes.rs`) e consultas seguras (`consulta.rs`) para o construtor de
//! relatórios e, depois, para o BI e a verificação da qualidade dos dados. Só lê: as alterações continuam a passar
//! pelas operações de cada módulo (com as suas permissões e a auditoria).

mod consulta;
mod fontes;

pub use consulta::{consultar, fontes_listar};
