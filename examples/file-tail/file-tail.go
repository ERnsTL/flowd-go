package main

import (
	"bufio"
	"flag"
	"fmt"
	"os"
	"strings"

	"github.com/ERnsTL/flowd/libflowd"
	"github.com/hpcloud/tail"
)

func main() {
	// open connection to network
	netin := bufio.NewReader(os.Stdin)
	netout := bufio.NewWriter(os.Stdout)
	defer netout.Flush()
	// flag variables
	var filePath string
	var debug, quiet bool
	// get configuration from IIP = initial information packet/frame
	fmt.Fprintln(os.Stderr, "wait for IIP")
	if iip, err := flowd.GetIIP("CONF", netin); err != nil {
		fmt.Fprintln(os.Stderr, "ERROR getting IIP:", err, "- Exiting.")
		os.Exit(1)
	} else {
		// parse IIP
		flags := flag.NewFlagSet("file-tail", flag.ContinueOnError)
		flags.BoolVar(&debug, "debug", false, "give detailed event output")
		flags.BoolVar(&quiet, "quiet", false, "no informational output except errors")
		if err := flags.Parse(strings.Split(iip, " ")); err != nil {
			os.Exit(2)
		}
		if flags.NArg() == 0 {
			fmt.Fprintln(os.Stderr, "ERROR: missing filepath to read")
			printUsage()
			flags.PrintDefaults() // prints to STDERR
			os.Exit(2)
		} else if flags.NArg() > 1 {
			fmt.Fprintln(os.Stderr, "ERROR: more than one filepath unimplemented")
			printUsage()
			flags.PrintDefaults() // prints to STDERR
			os.Exit(2)
		}
		filePath = flags.Args()[0]
	}
	if !debug {
		fmt.Fprintln(os.Stderr, "starting up")
	} else {
		fmt.Fprintf(os.Stderr, "starting up, filepath is %s", filePath)
	}

	// prepare variables
	outframe := &flowd.Frame{ //TODO why is this pointer to Frame?
		Type:     "data",
		BodyType: "FileLine",
		Port:     "OUT",
	}

	// start tailing
	t, err := tail.TailFile(filePath, tail.Config{Follow: true, Logger: tail.DiscardingLogger})
	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: in call to TailFile(): %s", err.Error())
		os.Exit(1)
	}

	// main work loop
	for line := range t.Lines {
		if debug {
			fmt.Fprintf(os.Stderr, "received line (%d bytes): %s", len([]byte(line.Text)), line.Text)
		}

		// save as body
		outframe.Body = []byte(line.Text)

		// send it to given output ports
		if err := outframe.Marshal(netout); err != nil {
			fmt.Fprintln(os.Stderr, "ERROR: marshaling frame:", err)
		}
	}

	//TODO add error handling on line.Err

	// report
	if debug {
		fmt.Fprintln(os.Stderr, "all done")
	}
}

func printUsage() {
	fmt.Fprintln(os.Stderr, "Usage: "+os.Args[0]+" [flags] [file-path]")
}
