use wasm_bindgen::{
    prelude::*,
    JsValue,
};

// thank you github.com/paulcdejean
#[wasm_bindgen]
extern "C" {
    pub type NS;

    #[wasm_bindgen(method)]
    fn print(
        this: &NS,
        print: &str,
    );

    #[wasm_bindgen(method)]
    fn clearLog(
        this: &NS,
    );

    #[wasm_bindgen(method)]
    fn tprint(
        this: &NS,
        print: &str,
    );

    #[wasm_bindgen(method)]
    fn read(
        ns: &NS,
        filename: &str,
    ) -> String;

    #[wasm_bindgen(method)]
    fn disableLog(
        ns: &NS,
        func: &str,
    );

    #[wasm_bindgen(method)]
    async fn sleep(
        ns: &NS,
        millis: u32,
    );

    #[wasm_bindgen(method)]
    fn resizeTail(
        ns: &NS,
        width: u32,
        height: u32,
    );
}

pub fn get_attribute<T>(
    object: &JsValue,
    field_name: &str,
    mapper: impl Fn(&JsValue) -> Option<T>,
) -> Result<Option<T>, JsValue> {
    js_sys::Reflect::get(object, &JsValue::from_str(field_name))
        .map(|x| mapper(&x))
}

#[wasm_bindgen]
pub async fn main_rs(ns: &NS) {
    use base64::engine::Engine as _;
    use std::io::BufRead as _;

    let args = get_attribute(ns, "args", |a| Some(js_sys::Array::from(a)))
        .unwrap()
        .unwrap();
    let mut args_iter = args.iter().map(|a| a.as_string().unwrap());

    ns.disableLog("ALL");

    let filename = args_iter.next().unwrap();

    // open a file
    let file_contents = ns.read(&filename);
    if file_contents.is_empty() {
        return;
    }

    // decode base64 then lz4
    let decoded = base64::prelude::BASE64_STANDARD.decode(&*file_contents);
    let decoded = decoded.unwrap();
    let decoder = std::io::BufReader::new(lz4_flex::frame::FrameDecoder::new(std::io::Cursor::new(decoded)));
    let mut decoder = decoder.lines();

    let framerate = decoder.next().unwrap().unwrap().parse::<f64>().unwrap();

    let dimensions = decoder.next().unwrap().unwrap();
    let mut dimensions = dimensions.split(" ");
    let x = dimensions.next().unwrap().parse::<u32>().unwrap();
    let y = dimensions.next().unwrap().parse::<u32>().unwrap();
 
    let mut buffer = String::new();
    let mut line_count = 0;

    let mut first_print = None;
    let mut frame_count = 0;

    for line in decoder {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                ns.tprint(&format!("{e:?}"));
                return;
            },
        };
        buffer += &line;
        buffer += "\n";
        line_count += 1;

        if line_count >= y {
            // sleep
            if let Some(first_print) = first_print.as_ref() {
                let next_time = &*first_print + frame_count as f64 / (framerate / 1000.);
                let now = js_sys::Date::now();

                ns.sleep((next_time - now).round() as u32).await;
            }

            else {
                first_print = Some(js_sys::Date::now());
            }

            // print
            ns.clearLog();
            ns.print(&buffer);
            ns.resizeTail(x * 10, y * 30 + 1);
            ns.resizeTail(x * 10, y * 30);
            buffer.clear();

            //buffer += "\u{001b}[0m\n";

            ns.tprint(&format!("frame {}", frame_count));
            frame_count += 1;
            line_count = 0;
        }
    }
}
