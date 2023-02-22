// This program starts a videotestsrc playing to an shmsink and
// a shmsrc to  autovideosink to show it on the screen.
//
// Then, another pipeline with an shmsrc to an appsink starts
// in a blocking task, tries to pull sample a fixed number of times,
// then sets itself to null.
//
// The observed problem is that after the shmsrc->appsink pipeline
// stops itself, something happens with the shmsrc->autovideosink pipeline
// a few seconds later, and the video display stops. I don't see anything iny
// my debug logs to indicate that either the videotestsrc or autovideosink
// pipelines have stopped.
//
// The video stops in the same place regardless of the framerate so it looks
// like it always stops a fixed number of frames later. This seems correlated
// with `shm-size` in shmsink
//
// Additionally, when the appsink pipeline is set to null, the one shmsink
// does not produce the client disconnected warning.
//
// Do shmsrc's propagate their state change back up to the shmsink?

use gst::prelude::*;
use std::str::FromStr;
use std::sync::mpsc::channel;

fn source() -> gst::Element {
    gst::parse_launch(
        "videotestsrc is-live=true pattern=ball !
         video/x-raw,format=I420,width=640,height=480,framerate=60/1 !
         identity silent=false dump=false !
         queue !
         shmsink name=source_shmsink socket-path=/tmp/pulldemo
         sync=false wait-for-connection=false
         ",
    )
    .unwrap()
}

fn display() -> gst::Element {
    gst::parse_launch(
        "shmsrc name=watch_shmsrc socket-path=/tmp/pulldemo is-live=true !
         video/x-raw,format=I420,width=640,height=480,framerate=60/1 !
         videoconvert !
         autovideosink sync=false",
    )
    .unwrap()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    gst::init()?;
    let shmsrc = gst::ElementFactory::make("shmsrc")
        .property("name", "appsink_shmsrc")
        .property("socket-path", "/tmp/pulldemo")
        .property("is-live", true)
        .build()?;
    let cf = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::caps::Caps::from_str(
                "video/x-raw,format=I420,width=640,height=480,framerate=60/1",
            )
            .unwrap(),
        )
        .build()?;
    let sink = gst_app::AppSink::builder()
        .name("appsink")
        .sync(false)
        .build();

    let mut count = 0;
    let (tx, rx) = channel();
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |appsink| {
                let res = appsink.pull_sample();
                match res {
                    Ok(_) => {
                        println!("Count {}", count);
                        count += 1;
                        let _ = tx.send(count);
                    }
                    Err(_) => {
                        println!("Done pulling from appsink.");
                        let _ = appsink.pull_sample();
                        return Err(gst::FlowError::Error);
                    }
                };
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    let src = source();
    let disp = display();
    src.set_state(gst::State::Playing)?;
    disp.set_state(gst::State::Playing)?;

    let pl = gst::Pipeline::new(None);
    let elems = [&shmsrc, &cf, sink.upcast_ref()];
    pl.add_many(&elems)?;
    gst::Element::link_many(&elems)?;

    pl.set_state(gst::State::Playing)?;

    let target = 100;
    let counter_task = std::thread::spawn(move || loop {
        let v = rx.recv().unwrap();
        if v == target {
            println!("Stopping appsink pipeline");
            pl.set_state(gst::State::Null).unwrap();
            return;
        }
    });

    println!("Setting appsink pipeline to playing");

    std::thread::sleep(std::time::Duration::from_secs(300));

    counter_task.join().unwrap();

    Ok(())
}
