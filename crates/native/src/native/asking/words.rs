//! The words that decide what asks, kept in one place to tune once real
//! sites and apps are measured: the verbs a control that sends is named
//! with, in English and a fixed set of other languages, all matched always,
//! and the keys that only move around.

/// Keys that only move: free wherever they land.
pub(super) const NAVIGATION: &[&str] = &[
    "up", "down", "left", "right", "tab", "escape", "pageup", "pagedown", "home", "end", "shift",
];

/// Verbs that name a control that sends, deletes or pays, matched at the start of any word of its name, ignoring case:
/// English, then German, French, Spanish, Italian, Portuguese, Dutch, Turkish, Russian, Ukrainian, Georgian and
/// Azerbaijani. Words in scripts written without spaces (Chinese, Japanese, Korean) are matched anywhere in the name.
const VERBS: &[&str] = &[
    // English.
    "send",
    "post",
    "reply",
    "comment",
    "submit",
    "merge",
    "confirm",
    "approve",
    "pay",
    "buy",
    "order",
    "purchase",
    "checkout",
    "transfer",
    "delete",
    "remove",
    "publish",
    "share",
    "invite",
    "tweet",
    // German.
    "senden",
    "absenden",
    "antworten",
    "kommentieren",
    "bestätigen",
    "genehmigen",
    "bezahlen",
    "kaufen",
    "bestellen",
    "überweisen",
    "löschen",
    "entfernen",
    "veröffentlichen",
    "teilen",
    "einladen",
    // French.
    "envoyer",
    "publier",
    "répondre",
    "commenter",
    "soumettre",
    "fusionner",
    "confirmer",
    "approuver",
    "payer",
    "acheter",
    "commander",
    "supprimer",
    "partager",
    "inviter",
    // Spanish.
    "enviar",
    "publicar",
    "responder",
    "comentar",
    "fusionar",
    "confirmar",
    "aprobar",
    "pagar",
    "comprar",
    "eliminar",
    "borrar",
    "compartir",
    "invitar",
    // Italian.
    "invia",
    "pubblica",
    "rispondi",
    "commenta",
    "conferma",
    "approva",
    "paga",
    "acquista",
    "elimina",
    "condividi",
    // Portuguese.
    "excluir",
    "apagar",
    // Dutch.
    "verzenden",
    "verstuur",
    "plaatsen",
    "beantwoorden",
    "bevestigen",
    "betalen",
    "kopen",
    "verwijderen",
    "delen",
    // Turkish.
    "gönder",
    "yayınla",
    "yanıtla",
    "onayla",
    "öde",
    "satın",
    "sil",
    "paylaş",
    // Russian and Ukrainian.
    "отправ",
    "опубликов",
    "ответ",
    "подтверд",
    "оплат",
    "купить",
    "удал",
    "надісл",
    "видал",
    // Georgian and Azerbaijani.
    "გაგზავნ",
    "წაშლ",
    "göndər",
    "sil",
    "təsdiq",
];

/// Verbs in scripts written without spaces, matched anywhere in a name.
const UNSPACED: &[&str] = &[
    "发送",
    "提交",
    "删除",
    "支付",
    "送信",
    "投稿",
    "削除",
    "購入",
    "보내기",
    "전송",
    "삭제",
    "결제",
];

/// Whether a control named `name` reads like one that sends.
pub(super) fn sends(name: &str) -> bool {
    let lowered = name.to_lowercase();
    lowered
        .split(|letter: char| !letter.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .any(|word| VERBS.iter().any(|verb| word.starts_with(verb)))
        || UNSPACED.iter().any(|verb| lowered.contains(verb))
}
