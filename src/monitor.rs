#![allow(unused_imports)]

use std::io;
use std::process;

use libc;
use tokio;
use tokio_io::IoFuture;

use futures::future::select_all;
use futures::{self, Future, Stream};

use plugin::Plugin;
use relay::boxed_future;

#[cfg(unix)]
pub fn monitor_signal(plugins: Vec<Plugin>) -> impl Future<Item = (), Error = io::Error> + Send {
    use tokio_signal::unix::Signal;

    // Monitor SIGCHLD, triggered if subprocess (plugin) is exited.
    let fut1 = Signal::new(libc::SIGCHLD).and_then(|signal| {
                                                       signal.take(1)
                                                             .for_each(|_| -> Result<(), io::Error> {
                                                                           error!("Plugin exited unexpectly (SIGCHLD)");
                                                                           Ok(())
                                                                       })
                                                             .map(|_| libc::SIGCHLD)
                                                   })
                                         .map_err(|err| {
                                                      error!("Failed to monitor SIGCHLD, err: {:?}", err);
                                                      err
                                                  });

    // Monitor SIGTERM, triggered if shadowsocks is exited gracefully. (Kill by user).
    let fut2 = Signal::new(libc::SIGTERM).and_then(|sigterm| {
                                                       sigterm.take(1)
                                                              .for_each(|_| -> Result<(), io::Error> {
                                                                            info!("Received SIGTERM, exiting.");
                                                                            Ok(())
                                                                        })
                                                              .map(|_| libc::SIGTERM)
                                                   })
                                         .map_err(|err| {
                                                      error!("Failed to monitor SIGTERM, err: {:?}", err);
                                                      err
                                                  });

    // Monitor SIGINT, triggered by CTRL-C
    let fut3 = Signal::new(libc::SIGINT).and_then(|sigint| {
                                                      sigint.take(1)
                                                            .for_each(|_| -> Result<(), io::Error> {
                                                                          info!("Received SIGINT, exiting.");
                                                                          Ok(())
                                                                      })
                                                            .map(|_| libc::SIGINT)
                                                  })
                                        .map_err(|err| {
                                                     error!("Failed to monitor SIGINT, err: {:?}", err);
                                                     err
                                                 });

    // Join them all, if any of them is triggered, kill all subprocesses and exit.
    fut1.select(fut2).then(|r| match r {
                  Ok((o, _)) => Ok(o),
                  Err((e, _)) => Err(e),
              })
        .select(fut3)
        .then(|r| match r {
                  Ok((o, _)) => Ok(o),
                  Err((e, _)) => Err(e),
              })
        .then(move |r| {
                  // Something happened ... killing all subprocesses
                  info!("Killing {} plugin(s) and then ... Bye Bye :)", plugins.len());
                  drop(plugins);

                  match r {
                      Ok(..) => {
                          process::exit(0);
                      }
                      Err(err) => Err(err),
                  }
              })
}

#[cfg(windows)]
pub fn monitor_signal(plugins: Vec<Plugin>) -> impl Future<Item = (), Error = io::Error> + Send {
    // FIXME: How to handle SIGTERM equavalent in Windows?

    use tokio_signal::windows::Event;

    let fut1 = Event::ctrl_c().and_then(|ev| {
                                            ev.take(1).for_each(|_| -> Result<(), io::Error> {
                                                                    error!("Received Ctrl-C event");
                                                                    Ok(())
                                                                })
                                        })
                              .map_err(|err| {
                                           error!("Failed to monitor Ctrl-C event: {:?}", err);
                                           err
                                       });

    let fut2 = Event::ctrl_break().and_then(|ev| {
                                                ev.take(1).for_each(|_| -> Result<(), io::Error> {
                                                                        error!("Received Ctrl-Break event");
                                                                        Ok(())
                                                                    })
                                            })
                                  .map_err(|err| {
                                               error!("Failed to monitor Ctrl-Break event: {:?}", err);
                                               err
                                           });

    fut1.select(fut2).then(|_| -> Result<(), ()> {
                               // Something happened ... killing all subprocesses
                               info!("Killing {} plugin(s) and then ... Bye Bye :)", plugins.len());
                               drop(plugins);
                               process::exit(libc::EXIT_FAILURE);
                           })
}

#[cfg(not(any(windows, unix)))]
pub fn monitor_signal(plugins: Vec<Plugin>) -> impl Future<Item = (), Error = io::Error> + Send {
    // FIXME: What can I do ...
    // Blocks forever
    futures::empty::<(), ()>().and_then(|_| {
                                            drop(plugins);
                                        })
}
