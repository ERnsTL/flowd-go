package main

import (
	"bufio"
	"flag"
	"fmt"
	"io"
	"os"
	"regexp"
	"strings"

	"github.com/ERnsTL/flowd/libflowd"
)

func main() {
	// open connection to network
	netin := bufio.NewReader(os.Stdin)
	netout := bufio.NewWriter(os.Stdout)
	defer netout.Flush()
	// get configuration from IIP = initial information packet/frame
	var exp *regexp.Regexp
	var field string
	var debug bool
	fmt.Fprintln(os.Stderr, "wait for IIP")
	if iip, err := flowd.GetIIP("CONF", netin); err != nil {
		fmt.Fprintln(os.Stderr, "ERROR getting IIP:", err, "- Exiting.")
		os.Exit(1)
	} else {
		// parse IIP
		flags := flag.NewFlagSet(os.Args[0], flag.ContinueOnError)
		flags.StringVar(&field, "field", "myfield", "header field to set using matched subgroup")
		//flags.StringVar(&regexpStr, "exp", "marker ([0-9]+)", "regular expression for matching and extracting the field value")
		flags.BoolVar(&debug, "debug", false, "give detailed event output")
		//flags.BoolVar(&quiet, "quiet", false, "no informational output except errors")
		if err = flags.Parse(strings.Split(iip, " ")); err != nil {
			os.Exit(2)
		}
		if flags.NArg() == 0 {
			fmt.Fprintln(os.Stderr, "ERROR: missing regular expression")
			printUsage()
			flags.PrintDefaults() // prints to STDERR
			os.Exit(2)
		}
		// compile regular expression
		regexpStr := strings.Join(flags.Args(), " ") // in case regular expression contains space
		exp, err = regexp.Compile(regexpStr)
		if err != nil {
			fmt.Fprintln(os.Stderr, "ERROR: regular expression does not compile:", regexpStr)
			os.Exit(2)
		} else if debug {
			fmt.Fprintln(os.Stderr, "got regular expression:", regexpStr)
		}
	}
	fmt.Fprintln(os.Stderr, "reading packets")

	var frame *flowd.Frame
	var err error
	var matches [][]byte

	// read frames
	for {
		frame, err = flowd.ParseFrame(netin)
		if err != nil {
			if err == io.EOF {
				break
			}
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}

		// check for port close notification
		if frame.Type == "control" && frame.BodyType == "PortClose" {
			fmt.Fprintln(os.Stderr, "input port closed - exiting.")
			// done
			break
		}

		// check for match
		matches = exp.FindSubmatch(frame.Body) //TODO [0] = match of entire expression, [1..] = submatches / match groups
		if matches == nil {
			// no match, forward unmodified
			if debug {
				fmt.Fprintln(os.Stderr, "no match, forwarding unmodified:", string(frame.Body))
			}
		} else {
			if debug {
				fmt.Fprintf(os.Stderr, "got matches, modifying: %v\n", matches)
			}

			// modify frame
			if frame.Extensions == nil {
				if debug {
					fmt.Fprintln(os.Stderr, "frame extensions map is nil, initalizing")
				}
				frame.Extensions = map[string]string{}
			}
			frame.Extensions[field] = string(matches[1])
		}

		// send out modified frame
		frame.Port = "OUT"
		if err := frame.Marshal(netout); err != nil {
			fmt.Fprintln(os.Stderr, "ERROR: marshaling frame:", err.Error())
		}
	}

}

func printUsage() {
	fmt.Fprintln(os.Stderr, "Usage: "+os.Args[0]+" [flags] -field=[header-field] [reg-exp]")
	fmt.Fprintln(os.Stderr, "expression is free argument may contain space, do not quote")
}