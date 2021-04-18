use xtra_proc::actor;
use xtra_proc::handler;
use async_trait::async_trait;

#[actor]
struct MyActor {
    pub counter: usize
}

#[actor]
impl MyActor {
    pub fn new(init: usize) -> Self {
        Self {
            counter: init
        }
    }

    #[handler]
    pub async fn hello_world(&mut self, string: String) -> String {
        let string = format!("hello {}", string);

        println!("{}", string);

        string
    }

    #[handler]
    pub async fn method_two(&mut self, _arg1: u32, _arg2: String) -> Result<String, ()> {
        return Err(())
    }
}

#[tokio::main]
async fn main() {
    let actor = MyActor::new(&mut xtra::spawn::Tokio::Global, 123);

    actor.hello_world("123".into()).await;

}
