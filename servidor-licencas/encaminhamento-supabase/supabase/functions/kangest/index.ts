// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Encaminhamento temporário dos KanGest 1.4.0 para o servidor de licenças na Cloudflare (ver encaminhar.js).
import { encaminhar } from "./encaminhar.js";

Deno.serve((req: Request) => encaminhar(req));
