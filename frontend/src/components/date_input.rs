use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DateInputProps {
    pub value: Option<String>,
    pub onchange: Callback<Option<String>>,
    #[prop_or_default]
    pub label: Option<AttrValue>,
    #[prop_or_default]
    pub id: Option<AttrValue>,
    #[prop_or(false)]
    pub disabled: bool,
}

#[function_component(DateInput)]
pub fn date_input(props: &DateInputProps) -> Html {
    let on_input = {
        let onchange = props.onchange.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target().unwrap().dyn_into().unwrap();
            let val = input.value();
            if val.is_empty() {
                onchange.emit(None);
            } else {
                onchange.emit(Some(val));
            }
        })
    };

    let input_html = html! {
        <input
            type="date"
            id={props.id.clone()}
            value={props.value.clone().unwrap_or_default()}
            oninput={on_input}
            disabled={props.disabled}
        />
    };

    if let Some(ref label) = props.label {
        html! {
            <div class="form-group">
                <label for={props.id.clone()}>{ &**label }</label>
                { input_html }
            </div>
        }
    } else {
        input_html
    }
}
