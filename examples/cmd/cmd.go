package main

import (
	"bufio"
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"

	"github.com/ERnsTL/flowd/libflowd"
)

const bufSize = 2 ^ 16

type OperatingMode int

const (
	One OperatingMode = iota
	Each
)

// implement flag.Value interface
func (op *OperatingMode) String() string {
	op2 := *op //FIXME how to do this correctly? type switch?
	switch op2 {
	case One:
		return "one call handling all input IPs"
	case Each:
		return "one call for each input IP"
	default:
		if op == nil {
			return "nil"
		} else {
			return "ERROR value out of range"
		}
	}
}
func (op *OperatingMode) Set(value string) error {
	switch value {
	case "one":
		*op = One
	case "each":
		*op = Each
	default:
		return fmt.Errorf("set of allowable values for operating mode is {one, each}")
	}
	return nil
}

func main() {
	// open connection to network
	bufr := bufio.NewReader(os.Stdin)
	// flag variables
	var operatingMode OperatingMode
	var framing bool
	var retry bool
	var cmdargs []string
	var debug, quiet bool
	// get configuration from IIP = initial information packet/frame
	fmt.Fprintln(os.Stderr, "wait for IIP")
	if iip, err := flowd.GetIIP("CONF", bufr); err != nil {
		fmt.Fprintln(os.Stderr, "ERROR getting IIP:", err, "- Exiting.")
		os.Exit(1)
	} else {
		// parse IIP
		flags := flag.NewFlagSet("cmd", flag.ContinueOnError)
		flags.Var(&operatingMode, "mode", "operating mode: one (command instance handling all IPs) or each (IP handled by new instance)")
		flags.BoolVar(&framing, "framing", true, "true = frame mode, false = send frame body to command STDIN, frame the data from command STDOUT")
		flags.BoolVar(&retry, "retry", false, "retry/restart command on non-zero return code")
		flags.BoolVar(&debug, "debug", false, "give detailed event output")
		flags.BoolVar(&quiet, "quiet", false, "no informational output except errors")
		if err := flags.Parse(strings.Split(iip, " ")); err != nil {
			os.Exit(2)
		}
		if flags.NArg() == 0 {
			fmt.Fprintln(os.Stderr, "ERROR: missing command to run")
			printUsage()
			flags.PrintDefaults()
			os.Exit(2)
		}
		cmdargs = flags.Args()
	}
	//fmt.Fprintf(os.Stderr, "starting up in operating mode: %s, output framing: %t, retry: %t \n", operatingMode.String(), outframing, retry)
	fmt.Fprintln(os.Stderr, "starting up, command is", strings.Join(cmdargs, " "))

	// prepare subprocess variables
	var cmd *exec.Cmd
	var cin io.WriteCloser
	var cout io.ReadCloser

	//TODO implement timeout on subprocess
	/*
		// start
		cmd := exec.Command("sleep", "5")
		if err := cmd.Start(); err != nil {
			panic(err)
		}

		// wait or timeout
		donec := make(chan error, 1)
		go func() {
			donec <- cmd.Wait()
		}()
		select {
		case <-time.After(3 * time.Second):
			cmd.Process.Kill()
			fmt.Println("timeout")
		case <-donec:
			fmt.Println("done")
		}
	*/
	//TODO implement retry/restart in one mode
	//TODO implement retry/restart in each mode

	// main work loops
	switch operatingMode {
	case One:
		// start command as subprocess, with arguments
		cmd, cin, cout = startCommand(cmdargs)
		defer cout.Close()
		defer cin.Close()

		// handle subprocess output
		go handleCommandOutput(debug, cout)

		// handle subprocess input
		if framing == true {
			// setup direct copy without processing (since already framed)
			if _, err := io.Copy(cin, bufr); err != nil {
				fmt.Fprintln(os.Stderr, "ERROR: receiving from FBP network:", err, "Closing.")
				os.Stdin.Close()
				return
			}
		} else {
			// loop: read frame, write body to subprocess
			copyFrameBodies(cin, bufr)
		}
	case Each:
		// prepare variables
		var frame *flowd.Frame //TODO why is this pointer to Frame?
		var err error

		for {
			// read frame
			frame, err = flowd.ParseFrame(bufr)
			if err != nil {
				fmt.Fprintln(os.Stderr, err)
			} else if debug {
				fmt.Fprintln(os.Stderr, "received frame:", string(frame.Body))
			}

			// handle each frame with a new instance
			go func(frame *flowd.Frame) {
				// start new command instance
				cmd, cin, cout = startCommand(cmdargs)

				// handle subprocess output
				if framing {
					go handleCommandOutput(debug, cout)
				} else {
					go handleCommandOutputRaw(debug, cout)
				}

				// forward frame or frame body to subprocess
				if framing {
					// frame
					if err := frame.Marshal(cin); err != nil {
						fmt.Fprintln(os.Stderr, "ERROR: marshaling frame to command STDIN:", err, "- Exiting.")
						os.Exit(3)
					}
				} else {
					// frame body
					if _, err := cin.Write(frame.Body); err != nil {
						fmt.Fprintln(os.Stderr, "ERROR: writing frame body to command STDIN:", err, "- Exiting.")
						os.Exit(3)
					} else if debug {
						fmt.Fprintln(os.Stderr, "sent frame body to subcommand STDIN:", string(frame.Body))
					}
					// done sending = close command STDIN
					if err := cin.Close(); err != nil {
						fmt.Fprintln(os.Stderr, "ERROR: could not close command STDIN after writing frame:", err, "- Exiting.")
						os.Exit(3)
					}
				}

				// wait for subprocess to finish
				if err := cmd.Wait(); err != nil {
					fmt.Fprintln(os.Stderr, "ERROR: command exited with error:", err, "- Exiting.")
					os.Exit(3)
				}
			}(frame)
		}
	default:
		fmt.Fprintln(os.Stderr, "ERROR: main loop: unknown operating mode - Exiting.")
		os.Exit(3)
	}
}

func printUsage() {
	fmt.Fprintln(os.Stderr, "Usage: cmd [flags] [cmdpath] [args]...")
	os.Exit(2)
}

func startCommand(cmdargs []string) (cmd *exec.Cmd, cin io.WriteCloser, cout io.ReadCloser) {
	var err error
	cmd = exec.Command(cmdargs[0], cmdargs[1:]...)
	cout, err = cmd.StdoutPipe()
	if err != nil {
		fmt.Fprintln(os.Stderr, "ERROR: could not allocate pipe from command stdout:", err)
	}
	cin, err = cmd.StdinPipe()
	if err != nil {
		fmt.Fprintln(os.Stderr, "ERROR: could not allocate pipe to command stdin:", err)
	}
	cmd.Stderr = os.Stderr
	err = cmd.Start()
	if err != nil {
		fmt.Fprintln(os.Stderr, "ERROR: starting command:", err)
		os.Exit(3)
	}
	return
}

func handleCommandOutput(debug bool, cout io.ReadCloser) {
	bufr := bufio.NewReader(cout)
	for {
		if frame, err := flowd.ParseFrame(bufr); err != nil {
			if err == io.EOF {
				fmt.Fprintln(os.Stderr, "EOF from command stdout. Exiting.")
			} else {
				fmt.Fprintln(os.Stderr, "ERROR parsing frame from command stdout:", err, "- Exiting.")
			}
			cout.Close()
			return
		} else { // frame complete now
			if debug == true {
				fmt.Fprintln(os.Stderr, "STDOUT received frame type", frame.Type, "data type", frame.BodyType, "for port", frame.Port, "with body:", (string)(frame.Body)) //TODO what is difference between this and string(frame.Body) ?
			}
			// set correct port
			frame.Port = "OUT"
			// send into FBP network
			if err := frame.Marshal(os.Stdout); err != nil {
				fmt.Fprintln(os.Stderr, "ERROR: could not send frame to STDOUT:", err, "- Closing.")
				os.Stdout.Close()
				return
			}
		}
	}
}

func handleCommandOutputRaw(debug bool, cout io.ReadCloser) {
	// prepare readers and variables
	bufr := bufio.NewReader(cout)
	buf := make([]byte, bufSize)
	frame := &flowd.Frame{
		Port:     "OUT",
		Type:     "data",
		BodyType: "Data",
	}
	// read loop
	for {
		if nbytes, err := bufr.Read(buf); err != nil {
			if err == io.EOF {
				fmt.Fprintln(os.Stderr, "WARNING: EOF from command:", err, "- Closing.")
				cout.Close()
				return
			} else {
				fmt.Fprintln(os.Stderr, "ERROR: reading from command STDOUT:", err, "- Closing.")
				cout.Close()
				return
			}
		} else {
			// frame it and send into FBP network
			frame.Body = buf[0:nbytes]
			if err := frame.Marshal(os.Stdout); err != nil {
				fmt.Fprintln(os.Stderr, "ERROR: could not send frame to STDOUT:", err, "- Closing.")
				os.Stdout.Close()
				return
			}
		}
	}
}

func copyFrameBodies(cin io.WriteCloser, bufr *bufio.Reader) {
	var frame *flowd.Frame //TODO why is this pointer to Frame?
	var err error
	for {
		// read frame
		frame, err = flowd.ParseFrame(bufr)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
		}

		fmt.Fprintln(os.Stderr, "got packet:", string(frame.Body))

		// forward frame body to prepared command instance
		if _, err = cin.Write(frame.Body); err != nil {
			fmt.Fprintln(os.Stderr, "ERROR: writing frame body to command STDIN:", err, "- Exiting.")
			os.Exit(3)
		}
	}
}