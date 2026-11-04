//! Images pasted into a question form's note boxes: each box gets the composer's `[Image #N](path)`
//! reference, numbered across every box of the form, since they all reach the agent in one message.

use ghostex_gx_chat_core::query::answer_attachments_edit;
use serde_json::json;

#[test]
fn a_pasted_image_lands_at_the_caret_as_the_composers_reference() {
    let edit = answer_attachments_edit(&[
        json!("see this"),
        json!(["/data/i/1.png"]),
        json!("see this"),
        json!(8),
        json!(8),
    ])
    .expect("an edit");
    assert_eq!(edit.text, "see this [Image #1](/data/i/1.png) ");
    assert_eq!(edit.caret, edit.text.encode_utf16().count());
}

#[test]
fn numbers_go_on_from_the_forms_other_boxes() {
    let others = "[Image #1](/data/i/1.png) \n[Image #2](/data/i/2.png) ";
    let edit = answer_attachments_edit(&[
        json!(""),
        json!(["/data/i/3.png", "/data/i/4.png"]),
        json!(""),
        json!(0),
        json!(0),
        json!(others),
    ])
    .expect("an edit");
    assert_eq!(
        edit.text,
        "[Image #3](/data/i/3.png) [Image #4](/data/i/4.png) "
    );
}

#[test]
fn typing_during_the_upload_puts_the_reference_at_the_end() {
    let edit = answer_attachments_edit(&[
        json!("abc def"),
        json!(["/data/i/1.png"]),
        json!("abc"),
        json!(1),
        json!(1),
        json!(""),
    ])
    .expect("an edit");
    assert_eq!(edit.text, "abc def [Image #1](/data/i/1.png) ");
}
